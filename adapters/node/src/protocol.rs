use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::{adapter::AdapterMode, bdaddr::BDAddr, AdapterEvent, Callback, JsMessage, MsgRx, MsgTx, Result};

#[derive(Debug, Clone)]
pub struct NotifyData {
  address: BDAddr,
  data: Vec<u8>,
}

impl From<NotifyData> for AdapterEvent {
  fn from(notification: NotifyData) -> Self {
    Self::Data {
      device_id: notification.address.to_string(),
      data: notification.data.into(),
    }
  }
}

#[async_trait::async_trait]
pub trait Protocol: Send {
  async fn init(
    adapter_name: Option<String>,
    rx: MsgRx,
    callbacks: Vec<Callback>,
    cancel_token: CancellationToken,
  ) -> Result<Self>
  where
    Self: Sized;

  fn spawn(self) -> JoinHandle<()>;
}

struct ProtocolHandle {
  tx: MsgTx,
  _handle: JoinHandle<()>,
  cancel_token: CancellationToken,
}

enum Mode {
  Dual {
    ble: ProtocolHandle,
    rfcomm: ProtocolHandle,
  },
  Ble {
    handle: ProtocolHandle,
  },
  Rfcomm {
    handle: ProtocolHandle,
  },
}

pub struct ProtocolMan {
  mode: Mode,

  pub cancel_token: CancellationToken,
}

impl ProtocolMan {
  pub async fn init(adapter_name: Option<String>, mode: AdapterMode, callbacks: Vec<Callback>) -> Result<Self> {
    let cancel_token = CancellationToken::new();

    let mode = match mode {
      AdapterMode::Dual => {
        todo!();
      }
      AdapterMode::Ble => {
        todo!();
      }
      AdapterMode::Rfcomm => {
        let (tx, rx) = tokio::sync::mpsc::channel(16);
        let cancel_token = cancel_token.child_token();
        let protocol_cancel_token = cancel_token.clone();

        let handle = ProtocolHandle {
          tx,
          _handle: crate::rfcomm::get_rfcomm(adapter_name, rx, callbacks, protocol_cancel_token)
            .await?
            .spawn(),
          cancel_token,
        };

        Mode::Rfcomm { handle }
      }
    };

    Ok(Self { mode, cancel_token })
  }

  pub async fn send(&self, data: JsMessage) -> Result<()> {
    match &self.mode {
      Mode::Dual { ble, rfcomm } => {
        ble.tx.send(data.clone()).await?;
        rfcomm.tx.send(data).await?;
      }
      Mode::Ble { handle } => {
        handle.tx.send(data).await?;
      }
      Mode::Rfcomm { handle } => {
        handle.tx.send(data).await?;
      }
    }

    Ok(())
  }

  pub fn try_send(&self, data: JsMessage) -> Result<()> {
    match &self.mode {
      Mode::Dual { ble, rfcomm } => {
        ble.tx.try_send(data.clone())?;
        rfcomm.tx.try_send(data)?;
      }
      Mode::Ble { handle } => {
        handle.tx.try_send(data)?;
      }
      Mode::Rfcomm { handle } => {
        handle.tx.try_send(data)?;
      }
    }

    Ok(())
  }
}

impl Drop for ProtocolMan {
  fn drop(&mut self) {
    self.cancel_token.cancel();
  }
}
