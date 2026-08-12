use std::{
  collections::{HashMap, VecDeque},
  sync::{Arc, Mutex},
  thread,
  time::{SystemTime, UNIX_EPOCH},
};

use bridgething_companion::backend::{
  ActionSink, DismissReason, NotificationActionError, NotificationApp, NotificationBackend, NotificationCategory,
  NotificationFlags, NotificationInbox, NotificationRemoved, WireNotification,
};
use futures::StreamExt;
use tokio::sync::oneshot;
use zbus::{
  Connection, MatchRule, Message, MessageStream,
  fdo::MonitoringProxy,
  message::Type,
  zvariant::{OwnedValue, Value},
};

const NOTIFICATIONS: &str = "org.freedesktop.Notifications";
const NOTIFY: &str = "Notify";
const CLOSED: &str = "NotificationClosed";
const INVOKED: &str = "ActionInvoked";
const DESKTOP_ENTRY_HINT: &str = "desktop-entry";
const URGENCY_HINT: &str = "urgency";
const CATEGORY_HINT: &str = "category";
const URGENCY_NORMAL: u8 = 1;
const URGENCY_CRITICAL: u8 = 2;
const CLOSED_BY_USER: u32 = 2;
const IN_FLIGHT: usize = 64;
const MIRRORED: usize = 256;

type NotifyCall = (
  String,
  u32,
  String,
  String,
  String,
  Vec<String>,
  HashMap<String, OwnedValue>,
  i32,
);

#[derive(Default)]
pub struct FreedesktopNotifications {
  held: Mutex<Option<oneshot::Sender<()>>>,
}

impl NotificationBackend for FreedesktopNotifications {
  fn start(&self, inbox: Arc<NotificationInbox>) {
    self.stop();

    let (stop, halted) = oneshot::channel();
    match thread::Builder::new()
      .name("bridgething-notifications".to_owned())
      .spawn(move || watch(inbox, halted))
    {
      Ok(_) => *self.held.lock().unwrap() = Some(stop),
      Err(error) => tracing::warn!(%error, "the notification mirror could not be started"),
    }
  }

  fn stop(&self) {
    self.held.lock().unwrap().take();
  }

  fn invoke_positive(&self, _id: String, sink: Arc<ActionSink>) {
    sink.fail(NotificationActionError::NoTarget);
  }

  fn invoke_negative(&self, _id: String, sink: Arc<ActionSink>) {
    sink.fail(NotificationActionError::NoTarget);
  }
}

fn watch(inbox: Arc<NotificationInbox>, halted: oneshot::Receiver<()>) {
  match tokio::runtime::Builder::new_current_thread().enable_all().build() {
    Ok(runtime) => runtime.block_on(observe(inbox, halted)),
    Err(error) => tracing::warn!(%error, "the notification mirror has no runtime"),
  }
}

async fn observe(inbox: Arc<NotificationInbox>, mut halted: oneshot::Receiver<()>) {
  let connection = match Connection::session().await {
    Ok(connection) => connection,
    Err(error) => return tracing::warn!(%error, "there is no session bus to mirror notifications from"),
  };
  if let Err(error) = eavesdrop(&connection).await {
    return tracing::warn!(%error, "the session bus refused to hand out other apps notifications");
  }

  let mut messages = MessageStream::from(&connection);
  let mut mirror = Mirror::default();
  loop {
    tokio::select! {
      _ = &mut halted => break,
      message = messages.next() => {
        let Some(message) = message else { break };
        match message {
          Ok(message) => absorb(&mut mirror, &inbox, &message),
          Err(error) => tracing::debug!(%error, "the session bus sent a frame that could not be read"),
        }
      }
    }
  }
}

async fn eavesdrop(connection: &Connection) -> zbus::Result<()> {
  let rules = [
    MatchRule::builder()
      .msg_type(Type::MethodCall)
      .interface(NOTIFICATIONS)?
      .member(NOTIFY)?
      .build(),
    MatchRule::builder()
      .msg_type(Type::MethodReturn)
      .sender(NOTIFICATIONS)?
      .build(),
    MatchRule::builder()
      .msg_type(Type::Signal)
      .sender(NOTIFICATIONS)?
      .interface(NOTIFICATIONS)?
      .build(),
  ];
  MonitoringProxy::new(connection)
    .await?
    .become_monitor(&rules, 0)
    .await?;
  Ok(())
}

fn absorb(mirror: &mut Mirror, inbox: &NotificationInbox, message: &Message) {
  let header = message.header();
  let member = header.member().map(|member| member.as_str());
  let body = message.body();

  match message.message_type() {
    Type::MethodCall if member == Some(NOTIFY) => {
      let Some(caller) = header.sender() else { return };
      match body.deserialize::<NotifyCall>() {
        Ok(call) => mirror.called(caller.as_str(), header.primary().serial_num().get(), draft(call)),
        Err(error) => tracing::debug!(%error, "an app posted a notification in a shape the spec does not describe"),
      }
    }
    Type::MethodReturn => {
      let (Some(caller), Some(serial)) = (header.destination(), header.reply_serial()) else {
        return;
      };
      let Ok(id) = body.deserialize::<u32>() else { return };
      if let Some(posted) = mirror.answered(caller.as_str(), serial.get(), id) {
        inbox.on_posted(posted);
      }
    }
    Type::Signal if member == Some(INVOKED) => {
      if let Ok((id, _)) = body.deserialize::<(u32, String)>() {
        mirror.acted(id);
      }
    }
    Type::Signal if member == Some(CLOSED) => {
      if let Ok((id, reason)) = body.deserialize::<(u32, u32)>()
        && let Some(removed) = mirror.closed(id, reason)
      {
        inbox.on_removed(removed);
      }
    }
    _ => {}
  }
}

#[derive(Default)]
struct Mirror {
  calls: VecDeque<(String, u32, WireNotification)>,
  live: VecDeque<(u32, bool)>,
}

impl Mirror {
  fn called(&mut self, caller: &str, serial: u32, draft: WireNotification) {
    if self.calls.len() >= IN_FLIGHT {
      self.calls.pop_front();
    }
    self.calls.push_back((caller.to_owned(), serial, draft));
  }

  fn answered(&mut self, caller: &str, serial: u32, id: u32) -> Option<WireNotification> {
    let at = self
      .calls
      .iter()
      .position(|(who, sent, _)| who == caller && *sent == serial)?;
    let (_, _, mut posted) = self.calls.remove(at)?;
    posted.id = id.to_string();
    self.track(id);
    Some(posted)
  }

  fn track(&mut self, id: u32) {
    if let Some(slot) = self.live.iter_mut().find(|(live, _)| *live == id) {
      slot.1 = false;
      return;
    }
    if self.live.len() >= MIRRORED {
      self.live.pop_front();
    }
    self.live.push_back((id, false));
  }

  fn acted(&mut self, id: u32) {
    if let Some(slot) = self.live.iter_mut().find(|(live, _)| *live == id) {
      slot.1 = true;
    }
  }

  fn closed(&mut self, id: u32, reason: u32) -> Option<NotificationRemoved> {
    let at = self.live.iter().position(|(live, _)| *live == id)?;
    let (_, acted) = self.live.remove(at)?;
    Some(NotificationRemoved {
      id: id.to_string(),
      reason: match (acted, reason) {
        (true, _) => DismissReason::Acted,
        (false, CLOSED_BY_USER) => DismissReason::UserDismissed,
        (false, _) => DismissReason::RemoteDismissed,
      },
    })
  }
}

fn draft(call: NotifyCall) -> WireNotification {
  let (app_name, _, _, summary, body, _, hints, _) = call;
  let urgency = read(&hints, URGENCY_HINT).unwrap_or(URGENCY_NORMAL);

  WireNotification {
    id: String::new(),
    app: NotificationApp {
      bundle_id: read::<&str>(&hints, DESKTOP_ENTRY_HINT)
        .map(str::to_owned)
        .unwrap_or_else(|| app_name.clone()),
      display_name: (!app_name.is_empty()).then_some(app_name),
      icon_asset_id: None,
    },
    category: category(read(&hints, CATEGORY_HINT).unwrap_or_default()),
    title: (!summary.is_empty()).then_some(summary),
    subtitle: None,
    message: (!body.is_empty()).then_some(body),
    timestamp_unix_s: Some(now()),
    flags: NotificationFlags {
      silent: urgency < URGENCY_NORMAL,
      important: urgency >= URGENCY_CRITICAL,
    },
    positive_action: None,
    negative_action: None,
  }
}

fn read<'a, T>(hints: &'a HashMap<String, OwnedValue>, key: &str) -> Option<T>
where
  T: TryFrom<&'a Value<'a>>,
  <T as TryFrom<&'a Value<'a>>>::Error: Into<zbus::zvariant::Error>,
{
  hints.get(key)?.downcast_ref::<T>().ok()
}

fn category(raw: &str) -> NotificationCategory {
  let (family, kind) = raw.split_once('.').unwrap_or((raw, ""));
  match (family, kind) {
    ("call", "unanswered" | "missed") => NotificationCategory::MissedCall,
    ("call", _) => NotificationCategory::IncomingCall,
    ("email", _) => NotificationCategory::Email,
    ("im" | "presence", _) => NotificationCategory::Social,
    ("event" | "reminder", _) => NotificationCategory::Schedule,
    _ => NotificationCategory::Other,
  }
}

fn now() -> u32 {
  SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .map(|since| since.as_secs().min(u64::from(u32::MAX)) as u32)
    .unwrap_or_default()
}

#[cfg(test)]
mod tests {
  use std::time::Duration;

  use bridgething_companion::backend::NotificationEvent;

  use super::*;

  const NOTIFICATIONS_PATH: &str = "/org/freedesktop/Notifications";
  const SETTLE: Duration = Duration::from_millis(200);
  const ARMED: Duration = Duration::from_secs(5);
  const STAND_IN_ID: u32 = 7;

  fn hint(value: impl Into<Value<'static>>) -> OwnedValue {
    OwnedValue::try_from(value.into()).expect("a hint the bus could carry")
  }

  fn call(hints: HashMap<String, OwnedValue>, actions: Vec<String>) -> NotifyCall {
    (
      "Fastmail".to_owned(),
      0,
      "mail-unread".to_owned(),
      "a new message".to_owned(),
      "from someone you know".to_owned(),
      actions,
      hints,
      -1,
    )
  }

  fn labels(actions: &[&str]) -> Vec<String> {
    actions.iter().map(|action| (*action).to_owned()).collect()
  }

  #[test]
  fn a_posted_notification_only_reaches_the_inbox_once_the_daemon_names_it() {
    let mut mirror = Mirror::default();
    mirror.called(":1.7", 12, draft(call(HashMap::new(), Vec::new())));

    assert!(
      mirror.answered(":1.9", 12, 4).is_none(),
      "a reply to some other caller is not this app's notification"
    );
    assert!(
      mirror.answered(":1.7", 13, 4).is_none(),
      "a reply to some other call is not this notification"
    );

    let posted = mirror.answered(":1.7", 12, 4).expect("the daemon answered the call");
    assert_eq!(posted.id, "4");
    assert_eq!(posted.title.as_deref(), Some("a new message"));
    assert_eq!(posted.message.as_deref(), Some("from someone you know"));
    assert_eq!(posted.app.bundle_id, "Fastmail");
  }

  #[test]
  fn a_close_for_a_notification_that_was_never_mirrored_is_dropped() {
    let mut mirror = Mirror::default();

    assert!(
      mirror.closed(4, CLOSED_BY_USER).is_none(),
      "the device never saw this notification arrive, so it has nothing to remove"
    );
  }

  #[test]
  fn how_a_notification_went_away_survives_the_trip_to_the_device() {
    let mut mirror = Mirror::default();
    for id in [4, 5, 6] {
      mirror.called(":1.7", id, draft(call(HashMap::new(), Vec::new())));
      mirror.answered(":1.7", id, id).expect("the daemon answered the call");
    }

    mirror.acted(5);

    assert_eq!(
      mirror
        .closed(4, CLOSED_BY_USER)
        .expect("a mirrored notification")
        .reason,
      DismissReason::UserDismissed
    );
    assert_eq!(
      mirror
        .closed(5, CLOSED_BY_USER)
        .expect("a mirrored notification")
        .reason,
      DismissReason::Acted,
      "a notification the user answered is not one the user swiped away"
    );
    assert_eq!(
      mirror.closed(6, 1).expect("a mirrored notification").reason,
      DismissReason::RemoteDismissed,
      "an expiry is the app taking its own notification back"
    );
    assert!(
      mirror.closed(4, CLOSED_BY_USER).is_none(),
      "a notification is only removed once"
    );
  }

  #[test]
  fn the_hints_an_app_sends_decide_how_the_device_files_a_notification() {
    let hints = HashMap::from([
      (URGENCY_HINT.to_owned(), hint(URGENCY_CRITICAL)),
      (CATEGORY_HINT.to_owned(), hint("call.incoming")),
      (DESKTOP_ENTRY_HINT.to_owned(), hint("org.gnome.Calls")),
    ]);
    let posted = draft(call(
      hints,
      labels(&["default", "", "answer", "answer", "reject", "decline"]),
    ));

    assert_eq!(posted.app.bundle_id, "org.gnome.Calls");
    assert_eq!(posted.app.display_name.as_deref(), Some("Fastmail"));
    assert_eq!(posted.category, NotificationCategory::IncomingCall);
    assert!(posted.flags.important && !posted.flags.silent);
    assert!(
      posted.positive_action.is_none() && posted.negative_action.is_none(),
      "a button this desktop cannot press is not a button the device is offered"
    );
  }

  #[test]
  fn an_app_that_sends_no_hints_still_mirrors() {
    let posted = draft(call(HashMap::new(), Vec::new()));

    assert_eq!(posted.category, NotificationCategory::Other);
    assert!(!posted.flags.important && !posted.flags.silent);
    assert!(posted.positive_action.is_none() && posted.negative_action.is_none());
    assert!(posted.timestamp_unix_s.is_some_and(|stamp| stamp > 0));
  }

  #[test]
  fn a_low_urgency_notification_is_the_one_that_does_not_interrupt() {
    let hints = HashMap::from([(URGENCY_HINT.to_owned(), hint(0u8))]);
    let posted = draft(call(hints, Vec::new()));

    assert!(posted.flags.silent && !posted.flags.important);
  }

  struct StandInDaemon;

  #[zbus::interface(name = "org.freedesktop.Notifications")]
  impl StandInDaemon {
    #[allow(clippy::too_many_arguments)]
    fn notify(
      &self,
      _app_name: String,
      _replaces_id: u32,
      _app_icon: String,
      _summary: String,
      _body: String,
      _actions: Vec<String>,
      _hints: HashMap<String, OwnedValue>,
      _expire_timeout: i32,
    ) -> u32 {
      STAND_IN_ID
    }
  }

  #[tokio::test]
  #[ignore = "needs a session bus: dbus-run-session -- cargo test -p bridgething-desktop"]
  async fn a_notification_on_the_session_bus_is_mirrored_from_post_to_dismissal() {
    let daemon = zbus::connection::Builder::session()
      .expect("a session bus")
      .name(NOTIFICATIONS)
      .expect("the notifications name is free on a private bus")
      .serve_at(NOTIFICATIONS_PATH, StandInDaemon)
      .expect("the notifications object")
      .build()
      .await
      .expect("a stand-in notification daemon");

    let backend = FreedesktopNotifications::default();
    let (inbox, mut events) = NotificationInbox::channel();
    backend.start(inbox);

    let caller = Connection::session().await.expect("a session bus");
    let call = (
      "Fastmail".to_owned(),
      0u32,
      String::new(),
      "a new message".to_owned(),
      "from someone you know".to_owned(),
      Vec::<String>::new(),
      HashMap::<String, OwnedValue>::new(),
      -1i32,
    );

    let armed = tokio::time::Instant::now() + ARMED;
    let posted = loop {
      assert!(
        tokio::time::Instant::now() < armed,
        "the monitor never saw another app post a notification"
      );
      caller
        .call_method(
          Some(NOTIFICATIONS),
          NOTIFICATIONS_PATH,
          Some(NOTIFICATIONS),
          NOTIFY,
          &call,
        )
        .await
        .expect("the stand-in daemon took the notification");
      if let Ok(Some(event)) = tokio::time::timeout(SETTLE, events.recv()).await {
        break event;
      }
    };

    let NotificationEvent::Posted(posted) = posted else {
      panic!("a notification going out is a post, not a removal")
    };
    assert_eq!(posted.id, STAND_IN_ID.to_string());
    assert_eq!(posted.title.as_deref(), Some("a new message"));
    assert_eq!(posted.app.bundle_id, "Fastmail");

    while tokio::time::timeout(SETTLE, events.recv()).await.is_ok() {}

    daemon
      .emit_signal(
        None::<()>,
        NOTIFICATIONS_PATH,
        NOTIFICATIONS,
        CLOSED,
        &(STAND_IN_ID, CLOSED_BY_USER),
      )
      .await
      .expect("the stand-in daemon announced the dismissal");

    let removed = tokio::time::timeout(ARMED, events.recv())
      .await
      .expect("the dismissal reached the inbox")
      .expect("an event");
    assert_eq!(
      removed,
      NotificationEvent::Removed(NotificationRemoved {
        id: STAND_IN_ID.to_string(),
        reason: DismissReason::UserDismissed,
      })
    );

    let (sink, answer) = ActionSink::channel();
    backend.invoke_positive(STAND_IN_ID.to_string(), sink);
    assert_eq!(
      answer.await.expect("the sink settles"),
      Some(NotificationActionError::NoTarget),
      "watching the bus does not let this desktop answer for the app that posted"
    );

    backend.stop();
  }

  #[test]
  fn a_daemon_that_never_answers_cannot_grow_the_mirror_without_bound() {
    let mut mirror = Mirror::default();
    for serial in 0..(IN_FLIGHT as u32 * 2) {
      mirror.called(":1.7", serial, draft(call(HashMap::new(), Vec::new())));
    }

    assert_eq!(mirror.calls.len(), IN_FLIGHT);
    assert!(
      mirror.answered(":1.7", 0, 1).is_none(),
      "the oldest unanswered call is the one dropped"
    );
    assert!(mirror.answered(":1.7", IN_FLIGHT as u32, 1).is_some());
  }
}
