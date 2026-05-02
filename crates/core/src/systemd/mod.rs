pub mod power;

#[cfg(feature = "systemd")]
use sd_notify::{NotifyState, booted, notify};

pub trait Notify<'a> {
  fn new() -> Self
  where
    Self: Sized;

  fn ready(&self, ready: bool, status: Option<&'a str>);
  fn status(&self, status: &'a str);
}

pub fn init_notifier<'a>() -> impl Notify<'a> + Sized {
  #[cfg(feature = "systemd")]
  let notifier = SystemdNotify::new();

  #[cfg(not(feature = "systemd"))]
  let notifier = DummyNotify::new();

  notifier
}

#[cfg(feature = "systemd")]
pub struct SystemdNotify;

#[cfg(feature = "systemd")]
impl<'a> Notify<'a> for SystemdNotify {
  fn new() -> Self {
    tracing::debug!("creating systemd notifier");

    if let Err(err) = match booted() {
      Ok(true) => notify(&[NotifyState::Status("starting...")]),
      _ => panic!("system was not booted with systemd! please disable the systemd feature."),
    } {
      tracing::error!("failed to notify systemd!! {:?}", err);
    }

    Self
  }

  fn ready(&self, ready: bool, status: Option<&'a str>) {
    tracing::debug!("setting ready to {ready} with status: {:?}", status);

    let mut messages = vec![NotifyState::Ready];
    if let Some(status) = status {
      messages.push(NotifyState::Status(status));
    };

    if let Err(err) = notify(&messages) {
      tracing::error!("failed to notify systemd!! {:?}", err);
    }
  }

  fn status(&self, status: &'a str) {
    tracing::debug!("setting status to: {status}");

    if let Err(err) = notify(&[NotifyState::Status(status)]) {
      tracing::error!("failed to notify systemd!! {:?}", err);
    }
  }
}

// explicitly allowed dead_code so production builds don't warn
#[allow(dead_code)]
pub struct DummyNotify;

impl<'a> Notify<'a> for DummyNotify {
  fn new() -> Self {
    tracing::debug!("creating dummy systemd notifier");

    Self
  }

  fn ready(&self, ready: bool, status: Option<&'a str>) {
    tracing::debug!("setting ready to {ready} with status: {:?}", status);
  }

  fn status(&self, status: &'a str) {
    tracing::debug!("setting status to: {status}");
  }
}
