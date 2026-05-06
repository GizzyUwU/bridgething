mod handle;
use handle::*;

mod asset;
mod audio;
mod authority;
mod capabilities;
mod chrome;
mod geo;
mod library;
mod net;
mod notifications;
mod phone;
mod player;
mod system;
mod time;
mod voice;
mod webapp;

use asset::*;
use audio::*;
use authority::*;
use capabilities::*;
use chrome::*;
use geo::*;
use libbridgething::{
  gateway::{GatewayToBridgeMsg, GatewayToBridgeMsgData},
  wire::MsgMeta,
};
use library::*;
use net::*;
use notifications::*;
use phone::*;
use player::*;
use system::*;
use time::*;
use voice::*;
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
      protocol,
      msg: GatewayToBridgeMsg { id, meta, data },
      ..
    } = data;

    if let MsgMeta::Response(meta_resp) = &meta
      && self
        .bluetooth
        .gateway_man
        .complete_pending(&meta_resp.request_id, data.clone())
    {
      return Ok(());
    }

    let handle = MsgHandle::new(self, id, meta, address, protocol);

    match data {
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
          tracing::warn!(
            "({:?}) stray response-shape arrival on Asset surface with no matching pending request; dropping",
            handle.address,
          );
        }
      }
      GatewayToBridgeMsgData::Audio(audio_msg) => {
        if let Some(event) = audio_msg.into_event() {
          tokio::spawn(async move { AudioHandler::new(handle).handle(event).await });
        }
      }
      GatewayToBridgeMsgData::Authority(auth_msg) => {
        if let Some(event) = auth_msg.into_event() {
          tokio::spawn(async move { AuthorityHandler::new(handle).handle(event).await });
        }
      }
      GatewayToBridgeMsgData::Capabilities(cap_msg) => {
        if let Some(event) = cap_msg.into_event() {
          tokio::spawn(async move { CapabilitiesHandler::new(handle).handle(event).await });
        }
      }
      GatewayToBridgeMsgData::Chrome(chrome_msg) => {
        if let Some(cmd) = chrome_msg.into_command() {
          tokio::spawn(async move { ChromeHandler::new(handle).handle(cmd).await });
        }
      }
      GatewayToBridgeMsgData::Geo(geo_msg) => {
        if let Some(event) = geo_msg.into_event() {
          tokio::spawn(async move { GeoHandler::new(handle).handle(event).await });
        }
      }
      GatewayToBridgeMsgData::Library(lib_msg) => {
        if let Some(event) = lib_msg.into_event() {
          tokio::spawn(async move { LibraryHandler::new(handle).handle(event).await });
        }
      }
      GatewayToBridgeMsgData::Net(net_msg) => {
        tokio::spawn(async move { NetHandler::new(handle).handle(net_msg).await });
      }
      GatewayToBridgeMsgData::Notifications(notif_msg) => {
        if let Some(event) = notif_msg.into_event() {
          tokio::spawn(async move { NotificationsHandler::new(handle).handle(event).await });
        }
      }
      GatewayToBridgeMsgData::Phone(phone_msg) => {
        if let Some(event) = phone_msg.into_event() {
          tokio::spawn(async move { PhoneHandler::new(handle).handle(event).await });
        }
      }
      GatewayToBridgeMsgData::Player(player_msg) => {
        if let Some(event) = player_msg.into_event() {
          tokio::spawn(async move { PlayerHandler::new(handle).handle_event(event).await });
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
      GatewayToBridgeMsgData::Time(time_msg) => {
        tokio::spawn(async move { TimeHandler::new(handle).handle(time_msg).await });
      }
      GatewayToBridgeMsgData::Voice(voice_msg) => {
        if let Some(cmd) = voice_msg.into_command() {
          tokio::spawn(async move { VoiceHandler::new(handle).handle(cmd).await });
        }
      }
      GatewayToBridgeMsgData::Webapp(webapp_msg) => {
        if let Some(req) = webapp_msg.into_request() {
          tokio::spawn(async move { WebappHandler::new(handle).handle(req).await });
        }
      }
      GatewayToBridgeMsgData::Error(err) => {
        tracing::warn!("({:?}) gateway reported a protocol error: {:?}", &handle.address, err);
      }
    }

    Ok(())
  }
}
