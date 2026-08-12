use std::sync::Arc;

use tokio::sync::mpsc;

#[uniffi::export(with_foreign)]
pub trait ConnectivityMonitor: Send + Sync {
  fn start(&self, inbox: Arc<ConnectivityInbox>);
  fn stop(&self);
}

#[derive(uniffi::Object)]
pub struct ConnectivityInbox {
  tx: mpsc::UnboundedSender<bool>,
}

impl ConnectivityInbox {
  pub fn channel() -> (Arc<Self>, mpsc::UnboundedReceiver<bool>) {
    let (tx, rx) = mpsc::unbounded_channel();
    (Arc::new(Self { tx }), rx)
  }
}

#[uniffi::export]
impl ConnectivityInbox {
  pub fn on_changed(&self, online: bool) {
    let _ = self.tx.send(online);
  }
}
