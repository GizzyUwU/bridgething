mod handle;
use handle::*;

mod chrome;
mod file;
mod webapp;

use chrome::*;
use file::*;
use webapp::*;

use libbridgething::{
  ForwardMessage, GatewayMeta, ServerEventType,
  gateway::{GatewayToBridgeMsg, GatewayToBridgeMsgData},
  server::{GatewayStatus, ServerSystemEvent},
};

use crate::{
  bluetooth::{BluetoothMan, GatewayMessage},
  state::State,
};

use super::HandlerResult;

pub struct GatewayHandler {
  state: State,
  bluetooth: BluetoothMan,
}

impl GatewayHandler {
  pub fn new(state: State, bluetooth: BluetoothMan) -> Self {
    Self { state, bluetooth }
  }

  pub async fn handle(&self, data: GatewayMessage<GatewayToBridgeMsg>) -> HandlerResult {
    tracing::trace!(
      "handling {:?} bluetooth event from {:?}: {:?}",
      data.protocol,
      data.address,
      data.msg
    );

    let GatewayMessage {
      address,
      protocol,
      msg: GatewayToBridgeMsg { id, meta, data },
    } = data;
    let handle = MsgHandle::new(self, id, meta, address, protocol);

    match data {
      GatewayToBridgeMsgData::Version(data) => {
        tokio::spawn(async move { TopLevelHandler::new(handle).handle_version(data).await });
      }
      GatewayToBridgeMsgData::File(file_msg) => {
        tokio::spawn(async move { FileHandler::new(handle).handle(file_msg).await });
      }
      GatewayToBridgeMsgData::Chrome(chrome_msg) => {
        tokio::spawn(async move { ChromeHandler::new(handle).handle(chrome_msg).await });
      }
      GatewayToBridgeMsgData::Webapp(webapp_msg) => {
        tokio::spawn(async move { WebappHandler::new(handle).handle(webapp_msg).await });
      }
      GatewayToBridgeMsgData::Forward(forward) => {
        tokio::spawn(async move { TopLevelHandler::new(handle).handle_forward(forward).await });
      }
      GatewayToBridgeMsgData::Error(err) => {
        tracing::warn!(
          "({:?}) gateway reported a protocol error: {:?}",
          &handle.address,
          err
        );
      }
    }

    Ok(())
  }
}

#[derive(Debug)]
struct TopLevelHandler {
  handle: MsgHandle,
}

impl TopLevelHandler {
  pub fn new(handle: MsgHandle) -> Self {
    Self { handle }
  }

  pub async fn handle_version(&mut self, version: GatewayMeta) -> HandlerResult {
    tracing::debug!("({:?}) version: {:?};", &self.handle.address, &version);

    let status = GatewayStatus {
      address: self.handle.address.unwrap_or_default().to_string(),
      connected: true,

      lib_version: version.lib_version,
      libbridgething_version: version.libbridgething_version,
      adapter_version: version.adapter_version,
      app_name: version.app_name,
      app_version: version.app_version,
      os_name: version.os_name,
    };
    self.handle.state.set_gateway_status(status.clone()).await?;

    Ok(())
  }

  pub async fn handle_forward(&mut self, data: ForwardMessage) -> HandlerResult {
    tracing::debug!("({:?}) handling forward message", &self.handle.address);

    self
      .handle
      .state
      .client_man
      .broadcast(data, ServerEventType::Event)
      .await?;

    Ok(())
  }
}
