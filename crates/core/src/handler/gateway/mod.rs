mod handle;
use handle::*;

mod chrome;
mod file;
mod webapp;

use chrome::*;
use file::*;
use libbridgething::{
  DeviceType, ForwardMessage, GatewayMeta, NowPlayingUpdate, PeerCompanionStatus, ServerEventType,
  gateway::{GatewayToBridgeMsg, GatewayToBridgeMsgData},
};
use webapp::*;

use super::HandlerResult;
use crate::{
  bluetooth::{BluetoothMan, GatewayMessage},
  state::State,
};

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
      priority,
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
      GatewayToBridgeMsgData::NowPlayingUpdate(update) => {
        tokio::spawn(async move { TopLevelHandler::new(handle).handle_now_playing(update).await });
      }
      GatewayToBridgeMsgData::Error(err) => {
        tracing::warn!("({:?}) gateway reported a protocol error: {:?}", &handle.address, err);
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
    if let Some(mac) = self.handle.address {
      let device_type = device_type_from_os(&version.os_name);
      if let Err(err) = self
        .handle
        .bluetooth
        .profile_man
        .upsert_paired_device(mac, device_type)
        .await
      {
        tracing::warn!(?err, "failed to upsert paired device on Version exchange");
      }
      let _ = self
        .handle
        .state
        .peers
        .set_companion(mac, PeerCompanionStatus::Connected(version))
        .await;
    }
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

  pub async fn handle_now_playing(&mut self, update: NowPlayingUpdate) -> HandlerResult {
    tracing::debug!("({:?}) handling now-playing delta from gateway", &self.handle.address);
    self.handle.state.player.apply_now_playing(update).await?;
    Ok(())
  }
}

fn device_type_from_os(os_name: &str) -> DeviceType {
  match os_name.to_ascii_lowercase().as_str() {
    "android" => DeviceType::Android,
    "ios" => DeviceType::Ios,
    "linux" => DeviceType::Linux,
    "macos" | "darwin" => DeviceType::MacOS,
    "windows" => DeviceType::Windows,
    _ => DeviceType::Unknown,
  }
}
