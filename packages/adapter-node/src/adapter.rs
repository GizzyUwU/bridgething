use std::sync::Arc;

use napi::{bindgen_prelude::*, threadsafe_function::UnknownReturnValue};

use crate::{Callbacks, Error, Event, JsMessage, Result, monitoring, protocol::ProtocolMan};

#[napi(string_enum)]
#[derive(Debug, Clone, Copy)]
pub enum AdapterMode {
  Dual,
  Ble,
  Rfcomm,
}

#[napi(object)]
#[derive(Debug)]
pub struct AdapterOptions {
  pub mode: Option<AdapterMode>,
  pub log_level_directive: Option<String>,
  pub adapter_name: Option<String>,
}

impl Default for AdapterOptions {
  fn default() -> Self {
    Self {
      mode: Some(AdapterMode::Rfcomm),
      log_level_directive: Some("bridgething_adapter=info".to_string()),
      adapter_name: None,
    }
  }
}

#[napi]
pub struct NodeAdapter {
  options: AdapterOptions,
  callback_purgatory: Option<Callbacks>,

  manager: Option<ProtocolMan>,
}

#[napi]
impl NodeAdapter {
  #[napi(constructor)]
  pub fn new(options: Option<AdapterOptions>) -> Self {
    let options = options.unwrap_or_default();
    monitoring::init_logger(options.log_level_directive.clone());

    Self {
      options,
      callback_purgatory: Some(Callbacks::default()),
      manager: None,
    }
  }

  #[napi]
  #[allow(clippy::missing_safety_doc)]
  pub async unsafe fn start(&mut self) -> Result<()> {
    if self.manager.is_some() {
      return Err(Error::AlreadyInitialized);
    };

    let manager = ProtocolMan::init(
      self.options.adapter_name.clone(),
      self.options.mode.unwrap_or(AdapterMode::Rfcomm),
      self.callback_purgatory.take().unwrap_or_default(),
    )
    .await?;

    self.manager = Some(manager);
    tracing::info!("started bridgething adapter - observing for car thing");

    Ok(())
  }

  #[napi]
  #[allow(clippy::missing_safety_doc)]
  pub async unsafe fn stop(&mut self) -> Result<()> {
    if let Some(manager) = self.manager.take() {
      manager.cancel_token.cancel();
    } else {
      return Err(Error::NotInitialized);
    }

    Ok(())
  }

  #[napi(ts_args_type = "callback: (event: AdapterEvent) => void")]
  pub fn on(&mut self, callback: Function<Event, UnknownReturnValue>) -> napi::Result<()> {
    tracing::debug!("registering new event listener callback");
    let callback = callback.build_threadsafe_function().build()?;

    if let Some(manager) = &self.manager {
      manager
        .try_send(callback.into())
        .map_err(|e| napi::Error::from_reason(e.to_string()))
    } else if let Some(purgatory) = self.callback_purgatory.as_mut() {
      purgatory.add(Arc::new(callback));
      Ok(())
    } else {
      tracing::error!("something went wrong - there was nowhere to send the callback!!");
      Ok(())
    }
  }

  #[napi]
  pub async fn disconnect(&self, device_id: String) -> napi::Result<()> {
    tracing::trace!("disconnect called with device_id: {device_id}");
    let device_id = device_id
      .parse()
      .map_err(|_| napi::Error::from_reason("failed to parse mac address".to_string()))?;

    Ok(self.forward(JsMessage::Disconnect(device_id)).await?)
  }

  #[napi]
  pub fn send(&self, device_id: String, frame: Buffer) -> napi::Result<()> {
    tracing::trace!("send called with device_id: {device_id} ({} bytes)", frame.len());
    let device_id = device_id
      .parse()
      .map_err(|_| napi::Error::from_reason("failed to parse mac address".to_string()))?;

    let bytes: Vec<u8> = frame.into();
    Ok(self.try_forward(JsMessage::Send(device_id, bytes))?)
  }

  async fn forward(&self, message: JsMessage) -> Result<()> {
    let manager = self.manager.as_ref().ok_or(Error::NotInitialized)?;
    manager.send(message).await
  }

  fn try_forward(&self, message: JsMessage) -> Result<()> {
    let manager = self.manager.as_ref().ok_or(Error::NotInitialized)?;
    manager.try_send(message)
  }
}

impl Drop for NodeAdapter {
  fn drop(&mut self) {
    if let Some(manager) = &self.manager {
      manager.cancel_token.cancel();
    }
  }
}
