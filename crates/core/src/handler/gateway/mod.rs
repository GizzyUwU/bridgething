mod handle;
use handle::*;

mod asset;
mod authority;
mod chrome;
mod system;
mod webapp;

use asset::*;
use authority::*;
use chrome::*;
use libbridgething::{
  DeviceType, GatewayMeta, NowPlayingUpdate, PeerCompanionStatus,
  gateway::{GatewayToBridgeMsg, GatewayToBridgeMsgData},
  wire::MsgMeta,
};
use system::*;
use webapp::*;

use super::HandlerResult;
use crate::{
  bluetooth::{BluetoothMan, InboundGatewayMessage},
  ota::OtaOrchestrator,
  state::State,
};

pub struct GatewayHandler {
  state: State,
  bluetooth: BluetoothMan,
  ota: OtaOrchestrator,
}

impl GatewayHandler {
  pub fn new(state: State, bluetooth: BluetoothMan, ota: OtaOrchestrator) -> Self {
    Self { state, bluetooth, ota }
  }

  pub async fn handle(&self, data: InboundGatewayMessage) -> HandlerResult {
    tracing::trace!(
      "handling {:?} bluetooth event from {:?}: {:?}",
      data.protocol,
      data.address,
      data.msg
    );

    let InboundGatewayMessage {
      address,
      msg: GatewayToBridgeMsg { id, meta, data },
      ..
    } = data;

    // Response messages may be completing a daemon-initiated typed
    // request (`gateway_man.request::<R>()`). Consume those here
    // before normal dispatch so the requester's awaiting future
    // resolves. If no pending request matches, fall through.
    if let MsgMeta::Response(meta_resp) = &meta
      && self
        .bluetooth
        .gateway_man
        .complete_pending(&meta_resp.request_id, data.clone())
    {
      return Ok(());
    }

    let handle = MsgHandle::new(self, id, meta, address);

    match data {
      GatewayToBridgeMsgData::Version(data) => {
        tokio::spawn(async move { TopLevelHandler::new(handle).handle_version(data).await });
      }
      GatewayToBridgeMsgData::Asset(asset_msg) => {
        if asset_msg.is_request_variant() {
          let req = asset_msg.into_request().expect("checked above");
          tokio::spawn(async move { AssetHandler::new(handle).handle_request(req).await });
        } else if asset_msg.is_event_variant() {
          let ev = asset_msg.into_event().expect("checked above");
          tokio::spawn(async move { AssetHandler::new(handle).handle_event(ev).await });
        } else if let Some(cmd) = asset_msg.into_command() {
          tokio::spawn(async move { AssetHandler::new(handle).handle_command(cmd).await });
        } else {
          // Stray response-shape arrival on the Asset surface: a
          // timed-out reply, a late reply, or a non-SDK companion
          // sending the response form without Response meta. The
          // pending-request match earlier consumed legitimate replies;
          // anything reaching here is a stray and gets warn-logged
          // and dropped.
          tracing::warn!(
            "({:?}) stray response-shape arrival on Asset surface with no matching pending request; dropping",
            handle.address,
          );
        }
      }
      GatewayToBridgeMsgData::Authority(auth_msg) => {
        if let Some(event) = auth_msg.into_event() {
          tokio::spawn(async move { AuthorityHandler::new(handle).handle(event).await });
        }
      }
      GatewayToBridgeMsgData::Chrome(chrome_msg) => {
        if let Some(cmd) = chrome_msg.into_command() {
          tokio::spawn(async move { ChromeHandler::new(handle).handle(cmd).await });
        }
      }
      GatewayToBridgeMsgData::System(system_msg) => {
        let ota = self.ota.clone();
        if system_msg.is_request_variant() {
          let req = system_msg.into_request().expect("checked above");
          tokio::spawn(async move { SystemHandler::new(handle, ota).handle_request(req).await });
        } else if system_msg.is_event_variant() {
          let ev = system_msg.into_event().expect("checked above");
          tokio::spawn(async move { SystemHandler::new(handle, ota).handle_event(ev).await });
        } else if let Some(cmd) = system_msg.into_command() {
          tokio::spawn(async move { SystemHandler::new(handle, ota).handle_command(cmd).await });
        }
      }
      GatewayToBridgeMsgData::Webapp(webapp_msg) => {
        if let Some(req) = webapp_msg.into_request() {
          tokio::spawn(async move { WebappHandler::new(handle).handle(req).await });
        }
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

  pub async fn handle_now_playing(&mut self, update: NowPlayingUpdate) -> HandlerResult {
    tracing::debug!("({:?}) handling now-playing delta from gateway", &self.handle.address);
    self
      .handle
      .state
      .player
      .apply_now_playing(crate::player::NowPlayingSource::Companion, update)
      .await?;
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
