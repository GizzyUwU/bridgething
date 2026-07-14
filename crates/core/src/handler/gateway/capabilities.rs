use libbridgething::{
  Device, DeviceType, GatewayCapabilities, PeerCompanionStatus,
  gateway::{BridgeToGatewayNotificationsMsgEvent, GatewayToBridgeCapabilitiesMsgEventDispatch},
};

use super::{HandlerResult, MsgHandle};
use crate::bluetooth::GatewayType;

pub struct CapabilitiesHandler {
  handle: MsgHandle,
}

impl CapabilitiesHandler {
  pub fn new(handle: MsgHandle) -> Self {
    Self { handle }
  }
}

impl GatewayToBridgeCapabilitiesMsgEventDispatch for CapabilitiesHandler {
  type Output = HandlerResult;

  async fn announce(&self, params: GatewayCapabilities) -> HandlerResult {
    if let Some(mac) = self.handle.address {
      let device = Device {
        name: params.gateway.name.clone(),
        device_type: device_type_from_os(&params.gateway.os_name),
        mac: mac.to_string(),
        default: false,
      };
      match self.handle.protocol {
        GatewayType::Network => {
          self.handle.state.peers.upsert(mac, device).await;
        }
        GatewayType::Rfcomm | GatewayType::Iap2Ea => {
          if let Some(profile_man) = self.handle.bluetooth.profile_man.try_get()
            && let Err(err) = profile_man.upsert_paired_device(mac, device.device_type.clone()).await
          {
            tracing::warn!(?err, "failed to upsert paired device on capabilities announce");
          }
          self.handle.state.peers.ensure_exists(mac, device).await;
        }
      }
      self
        .handle
        .state
        .peers
        .set_companion(mac, PeerCompanionStatus::Connected(params.gateway.clone()))
        .await;
      if let Err(err) = self.handle.state.capabilities.set_announce(mac, params).await {
        tracing::warn!(?err, "failed to publish capabilities snapshot");
      }
      let ancs = self.handle.bluetooth.le.ancs_auth_state();
      self
        .handle
        .bluetooth
        .gateway_man
        .send_event(mac, BridgeToGatewayNotificationsMsgEvent::AncsAuthStateChanged(ancs))
        .await;
    }
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
