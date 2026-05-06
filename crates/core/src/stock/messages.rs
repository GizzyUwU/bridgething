use libbridgething::{Device, NetworkKind};

use crate::{
  capabilities::CapabilitiesRegistry,
  net::{WSError, WireEventBus},
  stock::{
    StockConfigurationSend, StockConnectionSend, StockConnectionType, StockInterAppSend, StockInterAppSendPayload,
  },
};

/// Broadcasts that put the stock Spotify webapp into the "phone connected"
/// state. Fires whenever the peer's useful link comes up - first pair after
/// iAP2 identifies, RFCOMM gateway Version exchange, iAP2 reconnect, etc. -
/// not just on first BlueZ pair, since the webapp will sit on stale state
/// (and ignore now-playing deltas) if it doesn't see these on every link
/// transition.
pub async fn broadcast_stock_connection(
  bus: &WireEventBus,
  device: &Device,
  capabilities: &CapabilitiesRegistry,
) -> Result<(), Vec<WSError>> {
  bus
    .broadcast_stock(StockConnectionSend::RemoteStatus {
      payload: true,
      mac: device.mac.clone(),
      phone_type: device.device_type.clone().into(),
    })
    .await?;
  bus
    .broadcast_stock(StockConnectionSend::TransportStatus { payload: true })
    .await?;
  bus.broadcast_stock(StockConfigurationSend::default()).await?;

  let connection_type = match capabilities.snapshot().network.kind {
    NetworkKind::Wifi | NetworkKind::Ethernet => StockConnectionType::Wlan,
    NetworkKind::Cellular => StockConnectionType::FourG,
    NetworkKind::Unknown => StockConnectionType::Wlan,
  };
  bus
    .broadcast_stock(StockInterAppSend {
      msg_id: None,
      data: StockInterAppSendPayload::SessionState {
        connection_type,
        is_in_forced_offline_mode: false,
        is_logged_in: true,
        is_offline: false,
      },
    })
    .await?;

  bus
    .broadcast_stock(StockConnectionSend::RemoteApp {
      app_id: "com.bridgething".to_string(),
      is_spotify: true,
    })
    .await?;

  Ok(())
}

pub async fn broadcast_stock_disconnection(bus: &WireEventBus) -> Result<(), Vec<WSError>> {
  bus
    .broadcast_stock(StockConnectionSend::TransportStatus { payload: false })
    .await
}
