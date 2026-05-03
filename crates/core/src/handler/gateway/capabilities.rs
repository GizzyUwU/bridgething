use libbridgething::{DeviceType, PeerCompanionStatus, gateway::GatewayToBridgeCapabilitiesMsgEvent};

use super::{HandlerResult, MsgHandle};

pub struct CapabilitiesHandler {
  handle: MsgHandle,
}

impl CapabilitiesHandler {
  pub fn new(handle: MsgHandle) -> Self {
    Self { handle }
  }

  pub async fn handle(self, msg: GatewayToBridgeCapabilitiesMsgEvent) -> HandlerResult {
    match msg {
      GatewayToBridgeCapabilitiesMsgEvent::Announce(caps) => {
        let info = caps.gateway;
        if let Some(mac) = self.handle.address {
          let device_type = device_type_from_os(&info.os_name);
          if let Err(err) = self
            .handle
            .bluetooth
            .profile_man
            .upsert_paired_device(mac, device_type)
            .await
          {
            tracing::warn!(?err, "failed to upsert paired device on capabilities announce");
          }
          let _ = self
            .handle
            .state
            .peers
            .set_companion(mac, PeerCompanionStatus::Connected(info))
            .await;
        }
        Ok(())
      }
    }
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
