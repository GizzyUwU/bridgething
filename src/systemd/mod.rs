#[cfg(feature = "systemd")]
use sd_notify::{booted, notify, NotifyState};

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
impl Notify for SystemdNotify {
  fn new() -> Self {
    tracing::debug!("creating systemd notifier");

    let starting_res = match booted() {
      Some(true) => notify(false, &[NotifyState::Status("starting...")]),
      _ => panic("system was not booted with systemd! please disable the systemd feature."),
    };

    Self
  }

  fn ready(&self, ready: bool, status: Option<&'a str>) {
    tracing::debug!("setting ready to {ready} with status: {:?}", status);
  }

  fn status(&self, status: &'a str) {
    tracing::debug!("setting status to: {status}");
  }
}

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
