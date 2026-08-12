pub mod avahi;
pub mod power;
pub mod socket;
pub mod time;

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
pub struct SystemdNotify {
  booted: bool,
}

#[cfg(feature = "systemd")]
impl<'a> Notify<'a> for SystemdNotify {
  fn new() -> Self {
    tracing::debug!("creating systemd notifier");

    let booted = booted().unwrap_or(false);
    if !booted {
      tracing::warn!("host was not booted with systemd; readiness notifications are inert");
      return Self { booted };
    }

    if let Err(err) = notify(&[NotifyState::Status("starting...")]) {
      tracing::error!("failed to notify systemd!! {:?}", err);
    }

    Self { booted }
  }

  fn ready(&self, ready: bool, status: Option<&'a str>) {
    tracing::debug!("setting ready to {ready} with status: {:?}", status);
    if !self.booted {
      return;
    }

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
    if !self.booted {
      return;
    }

    if let Err(err) = notify(&[NotifyState::Status(status)]) {
      tracing::error!("failed to notify systemd!! {:?}", err);
    }
  }
}

#[cfg(not(feature = "systemd"))]
pub struct DummyNotify;

#[cfg(not(feature = "systemd"))]
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
