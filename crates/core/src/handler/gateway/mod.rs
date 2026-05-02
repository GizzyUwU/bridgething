mod handle;
use handle::*;

mod asset;
mod authority;
mod chrome;
mod webapp;

use asset::*;
use authority::*;
use chrome::*;
use libbridgething::{
  DeviceType, GatewayMeta, NowPlayingUpdate, PeerCompanionStatus,
  gateway::{GatewayMsgMeta, GatewayToBridgeMsg, GatewayToBridgeMsgData, is_response_variant},
};
use webapp::*;

use super::HandlerResult;
use crate::{
  bluetooth::{BluetoothMan, InboundGatewayMessage},
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
    if let GatewayMsgMeta::Response(meta_resp) = &meta
      && self
        .bluetooth
        .gateway_man
        .complete_pending(&meta_resp.request_id, data.clone())
    {
      return Ok(());
    }

    // Stray response shape that didn't match any pending request:
    // timed-out, late reply, or a non-SDK companion sending the
    // response form without response-meta. The per-surface dispatchers
    // are event-only by contract; let them stay event-only by filtering
    // these out here for every `impl_bridge_request!`-declared surface
    // at once.
    if is_response_variant(&data) {
      tracing::warn!(
        ?meta,
        ?data,
        "({:?}) stray response-shape arrival with no matching pending request; dropping",
        address,
      );
      return Ok(());
    }

    let handle = MsgHandle::new(self, id, meta, address);

    match data {
      GatewayToBridgeMsgData::Version(data) => {
        tokio::spawn(async move { TopLevelHandler::new(handle).handle_version(data).await });
      }
      GatewayToBridgeMsgData::Asset(asset_msg) => {
        if let Some(event) = asset_msg.into_event() {
          tokio::spawn(async move { AssetHandler::new(handle).handle(event).await });
        }
      }
      GatewayToBridgeMsgData::Authority(auth_msg) => {
        tokio::spawn(async move { AuthorityHandler::new(handle).handle(auth_msg).await });
      }
      GatewayToBridgeMsgData::Chrome(chrome_msg) => {
        tokio::spawn(async move { ChromeHandler::new(handle).handle(chrome_msg).await });
      }
      GatewayToBridgeMsgData::Webapp(webapp_msg) => {
        tokio::spawn(async move { WebappHandler::new(handle).handle(webapp_msg).await });
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
