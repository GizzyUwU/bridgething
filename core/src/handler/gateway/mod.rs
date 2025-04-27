mod handle;
use handle::*;

mod chrome;
mod file;

use chrome::*;
use file::*;

use libbridgething::gateway::{ArbitraryData, GatewayToBridgeMsg, GatewayToBridgeMsgData};

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
      GatewayToBridgeMsgData::Version { version, app } => {
        tokio::spawn(async move { TopLevelHandler::new(handle).handle_version(version, app).await });
      }
      GatewayToBridgeMsgData::File(file_msg) => {
        tokio::spawn(async move { FileHandler::new(handle).handle(file_msg).await });
      }
      GatewayToBridgeMsgData::Chrome(chrome_msg) => {
        tokio::spawn(async move { ChromeHandler::new(handle).handle(chrome_msg).await });
      }
      GatewayToBridgeMsgData::Data(arbitrary_data) => {
        tokio::spawn(async move { TopLevelHandler::new(handle).handle_data(arbitrary_data).await });
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
    tracing::debug!("({:?}) handling version message", &self.handle.address);
    tracing::debug!("({:?}) version: {:?}", &self.handle.address, version);
    tracing::debug!("({:?}) app: {:?}", &self.handle.address, app);

    Ok(())
  }

  pub async fn handle_data(&mut self, data: ArbitraryData) -> HandlerResult {
    tracing::debug!("({:?}) handling data message", &self.handle.address);
    tracing::debug!("({:?}) data: {:?}", &self.handle.address, data);

    Ok(())
  }
}
