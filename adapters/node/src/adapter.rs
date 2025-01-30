use napi::bindgen_prelude::*;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::{manager::BtMan, monitoring, Callback, Error, Event, JsMessage, MsgTx, Result};

#[derive(Debug)]
struct AdapterInner {
  tx: MsgTx,

  _handle: JoinHandle<Result<()>>,
  cancel_token: CancellationToken,
}

#[napi]
pub struct NodeAdapter {
  callback_purgatory: Option<Vec<Callback>>,
  inner: Option<AdapterInner>,
}

#[napi]
impl NodeAdapter {
  #[napi(constructor)]
  pub fn new(log_level_directive: Option<String>) -> Self {
    monitoring::init_logger(log_level_directive);
    Self::default()
  }

  #[napi]
  #[allow(clippy::missing_safety_doc)]
  pub async unsafe fn init(&mut self, adapter_name: Option<String>) -> Result<()> {
    if self.inner.is_some() {
      return Err(Error::AlreadyInitialized);
    };

    let (tx, rx) = tokio::sync::mpsc::channel(16);
    let cancel_token = CancellationToken::new();

    let bt_man = BtMan::new(
      adapter_name,
      rx,
      self.callback_purgatory.take().unwrap_or_default(),
      cancel_token.child_token(),
    )
    .await?;

    self.inner = Some(AdapterInner {
      tx,

      _handle: bt_man.spawn(),
      cancel_token,
    });

    tracing::info!("initialized bridgething adapter - scanning for car thing");

    Ok(())
  }

  #[napi(ts_args_type = "callback: (event: AdapterEvent) => void")]
  pub fn on(&mut self, callback: Function<Event>) -> napi::Result<()> {
    tracing::debug!("registering new event listener callback");
    let callback = callback.build_threadsafe_function().build()?;

    if let Some(inner) = self.inner.as_ref() {
      inner
        .tx
        .blocking_send(callback.into())
        .map_err(|e| napi::Error::from_reason(e.to_string()))
    } else if let Some(purgatory) = self.callback_purgatory.as_mut() {
      purgatory.push(callback);
      Ok(())
    } else {
      tracing::error!("something went wrong - there was nowhere to send the callback!!");
      Ok(())
    }
  }

  #[napi]
  pub async fn scan_on(&self) -> napi::Result<()> {
    Ok(self.forward(JsMessage::ScanOn).await?)
  }

  #[napi]
  pub async fn scan_off(&self) -> napi::Result<()> {
    Ok(self.forward(JsMessage::ScanOff).await?)
  }

  #[napi]
  pub async fn disconnect(&self, device_id: String) -> napi::Result<()> {
    let device_id = device_id
      .parse()
      .map_err(|_| napi::Error::from_reason("failed to parse mac address".to_string()))?;

    Ok(self.forward(JsMessage::Disconnect(device_id)).await?)
  }

  #[napi]
  pub async fn send(&self, device_id: String, message: Uint8Array) -> napi::Result<()> {
    let device_id = device_id
      .parse()
      .map_err(|_| napi::Error::from_reason("failed to parse mac address".to_string()))?;

    Ok(self.forward(JsMessage::Data(device_id, message.to_vec())).await?)
  }

  async fn forward(&self, message: JsMessage) -> Result<()> {
    let inner = self.inner.as_ref().ok_or(Error::NotInitialized)?;
    Ok(inner.tx.send(message).await?)
  }
}

impl Default for NodeAdapter {
  fn default() -> Self {
    Self {
      callback_purgatory: Some(Vec::new()),
      inner: None,
    }
  }
}

impl Drop for NodeAdapter {
  fn drop(&mut self) {
    if let Some(inner) = &self.inner {
      inner.cancel_token.cancel();
    }
  }
}
