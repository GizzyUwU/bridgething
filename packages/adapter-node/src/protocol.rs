use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::{Callbacks, JsMessage, MsgRx, MsgTx, Result, adapter::AdapterMode};

#[async_trait::async_trait]
pub trait Protocol: Send {
  async fn init(
    adapter_name: Option<String>,
    rx: MsgRx,
    callbacks: Callbacks,
    cancel_token: CancellationToken,
  ) -> Result<Self>
  where
    Self: Sized;

  fn spawn(self) -> JoinHandle<()>;
}

struct ProtocolHandle {
  tx: MsgTx,
  _handle: JoinHandle<()>,
}

pub struct ProtocolMan {
  rfcomm: ProtocolHandle,

  pub cancel_token: CancellationToken,
}

impl ProtocolMan {
  pub async fn init(adapter_name: Option<String>, mode: AdapterMode, callbacks: Callbacks) -> Result<Self> {
    tracing::debug!("initializing protocol manager");
    let cancel_token = CancellationToken::new();

    let rfcomm = match mode {
      AdapterMode::Rfcomm => Self::spawn_rfcomm(&cancel_token, adapter_name, callbacks).await?,
    };

    Ok(Self { rfcomm, cancel_token })
  }

  async fn spawn_rfcomm(
    parent: &CancellationToken,
    adapter_name: Option<String>,
    callbacks: Callbacks,
  ) -> Result<ProtocolHandle> {
    let (tx, rx) = tokio::sync::mpsc::channel(16);
    let _handle = crate::rfcomm::get_rfcomm(adapter_name, rx, callbacks, parent.child_token())
      .await?
      .spawn();
    Ok(ProtocolHandle { tx, _handle })
  }

  pub async fn send(&self, data: JsMessage) -> Result<()> {
    tracing::trace!("sending message: {data:?}");
    self.rfcomm.tx.send(data).await?;
    Ok(())
  }

  pub fn try_send(&self, data: JsMessage) -> Result<()> {
    tracing::trace!("try_sending message: {data:?}");
    self.rfcomm.tx.try_send(data)?;
    Ok(())
  }
}

impl Drop for ProtocolMan {
  fn drop(&mut self) {
    self.cancel_token.cancel();
  }
}
