use libbridgething::{
  Device, DeviceType, GatewayCapabilities, PeerCompanionStatus, gateway::GatewayToBridgeCapabilitiesMsgEventDispatch,
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
      let device_type = device_type_from_os(&params.gateway.os_name);
      match self.handle.protocol {
        GatewayType::Network => {
          let device = Device {
            name: params.gateway.name.clone(),
            device_type,
            mac: mac.to_string(),
            default: false,
          };
          self.handle.state.peers.upsert(mac, device).await;
        }
        GatewayType::Rfcomm | GatewayType::Iap2Ea => {
          if let Err(err) = self
            .handle
            .bluetooth
            .profile_man
            .get()
            .await
            .upsert_paired_device(mac, device_type)
            .await
          {
            tracing::warn!(?err, "failed to upsert paired device on capabilities announce");
          }
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
