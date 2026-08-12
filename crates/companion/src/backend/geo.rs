use std::sync::Arc;

use tokio::sync::mpsc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum GeoAccuracy {
  Coarse,
  Fine,
}

#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record)]
pub struct Position {
  pub lat: f64,
  pub lon: f64,
  pub alt_m: Option<f32>,
  pub accuracy_m: f32,
  pub speed_mps: Option<f32>,
  pub heading_deg: Option<f32>,
  pub ts_unix_s: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum GeoError {
  PermissionDenied,
  NotDeclared,
  Unavailable,
  UnknownToken,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GeoEvent {
  Position(Position),
  Failed(GeoError),
  AuthorizationChanged { granted: bool },
}

#[uniffi::export(with_foreign)]
pub trait GeoProvider: Send + Sync {
  fn can_provide_location(&self) -> bool;
  fn start(&self, inbox: Arc<GeoInbox>);
  fn stop(&self);
  fn configure(&self, accuracy: GeoAccuracy);
  fn request_authorization(&self);
  fn start_updating(&self);
  fn stop_updating(&self);
  fn request_once(&self);
  fn cancel_once(&self);
}

#[derive(uniffi::Object)]
pub struct GeoInbox {
  tx: mpsc::UnboundedSender<GeoEvent>,
}

impl GeoInbox {
  pub fn channel() -> (Arc<Self>, mpsc::UnboundedReceiver<GeoEvent>) {
    let (tx, rx) = mpsc::unbounded_channel();
    (Arc::new(Self { tx }), rx)
  }
}

#[uniffi::export]
impl GeoInbox {
  pub fn on_position(&self, position: Position) {
    let _ = self.tx.send(GeoEvent::Position(position));
  }

  pub fn on_error(&self, error: GeoError) {
    let _ = self.tx.send(GeoEvent::Failed(error));
  }

  pub fn on_authorization_change(&self, granted: bool) {
    let _ = self.tx.send(GeoEvent::AuthorizationChanged { granted });
  }
}
