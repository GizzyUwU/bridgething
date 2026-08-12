use std::sync::Arc;

use tokio::sync::mpsc;

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct LinkDevice {
  pub id: String,
  pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkEvent {
  Connected(LinkDevice),
  Disconnected {
    device_id: String,
  },
  LinkFailed {
    device_id: String,
    name: String,
    reason: String,
  },
  Bytes {
    device_id: String,
    bytes: Vec<u8>,
  },
  WriteComplete {
    device_id: String,
  },
  SendFailed {
    device_id: String,
  },
}

#[uniffi::export(with_foreign)]
pub trait LinkTransport: Send + Sync {
  fn max_batch_bytes(&self) -> u32;
  fn start(&self, inbox: Arc<LinkInbox>);
  fn stop(&self);
  fn send(&self, device_id: String, batch: Vec<u8>);
  fn disconnect(&self, device_id: String);
  fn reconnect(&self, device_id: String);
}

#[derive(uniffi::Object)]
pub struct LinkInbox {
  tx: mpsc::UnboundedSender<LinkEvent>,
}

impl LinkInbox {
  pub fn channel() -> (Arc<Self>, mpsc::UnboundedReceiver<LinkEvent>) {
    let (tx, rx) = mpsc::unbounded_channel();
    (Arc::new(Self { tx }), rx)
  }
}

#[uniffi::export]
impl LinkInbox {
  pub fn on_connected(&self, device: LinkDevice) {
    let _ = self.tx.send(LinkEvent::Connected(device));
  }

  pub fn on_disconnected(&self, device_id: String) {
    let _ = self.tx.send(LinkEvent::Disconnected { device_id });
  }

  pub fn on_link_failed(&self, device_id: String, name: String, reason: String) {
    let _ = self.tx.send(LinkEvent::LinkFailed {
      device_id,
      name,
      reason,
    });
  }

  pub fn on_bytes(&self, device_id: String, bytes: Vec<u8>) {
    let _ = self.tx.send(LinkEvent::Bytes { device_id, bytes });
  }

  pub fn on_write_complete(&self, device_id: String) {
    let _ = self.tx.send(LinkEvent::WriteComplete { device_id });
  }

  pub fn on_send_failed(&self, device_id: String) {
    let _ = self.tx.send(LinkEvent::SendFailed { device_id });
  }
}
