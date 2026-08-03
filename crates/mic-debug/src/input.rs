use std::{
  path::{Path, PathBuf},
  time::Duration,
};

use evdev::{Device, EventType, KeyCode};
use serde::{Deserialize, Serialize};
use tokio::{sync::mpsc, time::Instant};

use crate::status::Shared;

pub const STOP_HOLD: Duration = Duration::from_secs(2);
const RETRY_BACKOFF: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MarkKind {
  Utterance,
  FalseAlarm,
  Miss,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
  Mark(MarkKind),
  CycleTag,
  StartSession,
  StopSession,
}

pub async fn listen(commands: mpsc::Sender<Command>, shared: Shared) {
  loop {
    match find_gpio_keys().await {
      Some(path) => {
        tracing::info!(node = %path.display(), "buttons: listening");
        if let Err(err) = run(&path, &commands, &shared).await {
          tracing::warn!("button loop on {} ended: {err}", path.display());
        }
      }
      None => tracing::debug!("buttons: no gpio-keys node yet, retrying"),
    }
    tokio::time::sleep(RETRY_BACKOFF).await;
  }
}

async fn find_gpio_keys() -> Option<PathBuf> {
  let mut entries = tokio::fs::read_dir("/dev/input").await.ok()?;
  while let Ok(Some(entry)) = entries.next_entry().await {
    let path = entry.path();
    if !path
      .file_name()
      .and_then(|n| n.to_str())
      .is_some_and(|n| n.starts_with("event"))
    {
      continue;
    }
    let probe = path.clone();
    if let Ok(Some(name)) = tokio::task::spawn_blocking(move || open_name(&probe)).await
      && name.contains("gpio-keys")
    {
      return Some(path);
    }
  }
  None
}

fn open_name(path: &Path) -> Option<String> {
  Some(Device::open(path).ok()?.name().unwrap_or("").to_string())
}

async fn run(path: &Path, commands: &mpsc::Sender<Command>, shared: &Shared) -> Result<(), String> {
  let device = Device::open(path).map_err(|err| format!("open: {err}"))?;
  let mut events = device.into_event_stream().map_err(|err| format!("stream: {err}"))?;
  let mut stop_at: Option<Instant> = None;

  loop {
    tokio::select! {
      () = hold_progress(stop_at, shared) => {
        stop_at = None;
        shared.update(|status| status.stop_hold = 0.0);
        let _ = commands.send(Command::StopSession).await;
      }
      event = events.next_event() => {
        let event = event.map_err(|err| format!("read: {err}"))?;
        if event.event_type() != EventType::KEY {
          continue;
        }
        let key = KeyCode::new(event.code());
        match (key, event.value()) {
          (KeyCode::KEY_ESC, 1) => stop_at = Some(Instant::now() + STOP_HOLD),
          (KeyCode::KEY_ESC, 0) => {
            stop_at = None;
            shared.update(|status| status.stop_hold = 0.0);
          }
          (_, 1) => {
            if let Some(command) = press(key) {
              let _ = commands.send(command).await;
            }
          }
          _ => {}
        }
      }
    }
  }
}

fn press(key: KeyCode) -> Option<Command> {
  match key {
    KeyCode::KEY_1 | KeyCode::KEY_ENTER => Some(Command::Mark(MarkKind::Utterance)),
    KeyCode::KEY_2 => Some(Command::Mark(MarkKind::FalseAlarm)),
    KeyCode::KEY_3 => Some(Command::Mark(MarkKind::Miss)),
    KeyCode::KEY_4 => Some(Command::CycleTag),
    _ => None,
  }
}

async fn hold_progress(stop_at: Option<Instant>, shared: &Shared) {
  let Some(deadline) = stop_at else {
    return std::future::pending().await;
  };
  let mut tick = tokio::time::interval(Duration::from_millis(100));
  loop {
    tick.tick().await;
    let now = Instant::now();
    if now >= deadline {
      return;
    }
    let left = deadline.saturating_duration_since(now).as_secs_f32();
    shared.update(|status| status.stop_hold = 1.0 - (left / STOP_HOLD.as_secs_f32()));
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn the_wheel_click_marks_the_same_thing_preset_one_does() {
    assert_eq!(press(KeyCode::KEY_ENTER), press(KeyCode::KEY_1));
    assert_eq!(press(KeyCode::KEY_1), Some(Command::Mark(MarkKind::Utterance)));
  }

  #[test]
  fn each_preset_marks_a_distinct_outcome() {
    assert_eq!(press(KeyCode::KEY_2), Some(Command::Mark(MarkKind::FalseAlarm)));
    assert_eq!(press(KeyCode::KEY_3), Some(Command::Mark(MarkKind::Miss)));
    assert_eq!(press(KeyCode::KEY_4), Some(Command::CycleTag));
  }

  #[test]
  fn back_is_not_a_press_action_because_it_has_to_be_held() {
    assert_eq!(press(KeyCode::KEY_ESC), None);
  }

  #[test]
  fn the_mute_button_does_nothing_here() {
    assert_eq!(press(KeyCode::KEY_M), None);
  }
}
