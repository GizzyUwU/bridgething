mod handle;
use handle::*;

mod chrome;
mod file;

use chrome::*;
use file::*;

use libbridgething::{
  ForwardMessage, ServerEventType,
  gateway::{GatewayToBridgeMsg, GatewayToBridgeMsgData},
  server::ServerSystemEvent,
};

use crate::{
  bluetooth::{BluetoothMan, GatewayMessage},
  state::{GatewayStatus, State},
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
      GatewayToBridgeMsgData::Version { version, app } => {
        tokio::spawn(async move { TopLevelHandler::new(handle).handle_version(version, app).await });
      }
      GatewayToBridgeMsgData::File(file_msg) => {
        tokio::spawn(async move { FileHandler::new(handle).handle(file_msg).await });
      }
      GatewayToBridgeMsgData::Chrome(chrome_msg) => {
        tokio::spawn(async move { ChromeHandler::new(handle).handle(chrome_msg).await });
      }
      GatewayToBridgeMsgData::Forward(forward) => {
        tokio::spawn(async move { TopLevelHandler::new(handle).handle_forward(forward).await });
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

  pub async fn handle_version(&mut self, version: String, app: String) -> HandlerResult {
    tracing::debug!("({:?}) version: {:?};  app: {:?}", &self.handle.address, version, app);
    let status = GatewayStatus {
      address: self.handle.address.unwrap_or_default(),
      connected: true,
      version,
      app,
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
      .broadcast(data, ServerEventType::Gateway)
      .await?;

    Ok(())
  }
}
