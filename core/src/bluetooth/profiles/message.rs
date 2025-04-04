use libbridgething::{Device, ServerEventType, server::ServerBluetoothEvent};

use crate::{
  bluetooth::BluetoothResult,
  msg::stock::{
    StockConfigurationSend, StockConnectionSend, StockInterAppSend, StockInterAppSendPayload, StockSetupSend,
  },
  state::State,
};

pub async fn connection_messages(state: &State, new_device: bool, device: &Device) -> BluetoothResult<()> {
  if new_device {
    state
      .client_man
      .broadcast(
        ServerBluetoothEvent::ParingResult { success: true },
        ServerEventType::Info,
      )
      .await?;
  }

  state
    .client_man
    .broadcast(ServerBluetoothEvent::Status { connected: true }, ServerEventType::Info)
    .await?;
  state
    .client_man
    .broadcast(
      ServerBluetoothEvent::PairedDevices(state.get_devices().await),
      ServerEventType::Info,
    )
    .await?;
  state
    .client_man
    .broadcast(
      ServerBluetoothEvent::ConnectedDevice {
        name: device.name.clone(),
        mac: device.mac.clone(),
      },
      ServerEventType::Info,
    )
    .await?;

  state
    .client_man
    .broadcast_stock(StockConnectionSend::RemoteStatus {
      payload: true,
      mac: device.mac.clone(),
      phone_type: device.device_type.clone().into(),
    })
    .await?;
  state
    .client_man
    .broadcast_stock(StockConnectionSend::TransportStatus { payload: true })
    .await?;
  state
    .client_man
    .broadcast_stock(StockConfigurationSend::default())
    .await?;

  if new_device {
    state
      .client_man
      .broadcast_stock(StockSetupSend::Status {
        payload: "finished".to_string(),
      })
      .await?;
  }

  state
    .client_man
    .broadcast_stock(StockInterAppSend {
      msg_id: None,
      data: StockInterAppSendPayload::SessionState {
        connection_type: crate::msg::stock::StockConnectionType::FourG,
        is_in_forced_offline_mode: false,
        is_logged_in: true,
        is_offline: false,
      },
    })
    .await?;

  // TODO: remove testing code
  state
    .client_man
    .broadcast_stock(StockConnectionSend::RemoteApp {
      app_id: "com.bridgething".to_string(),
      is_spotify: true,
    })
    .await?;

  state.player.send_state().await?;

  // TODO: remove testing code
  // #[cfg(debug_assertions)]
  // state.client_man.broadcast(PlayerSend::dummy(), SendMsgMeta::Info).await?;
  // #[cfg(debug_assertions)]
  // state.client_man.broadcast(PlayerSend::dummy_queue(), SendMsgMeta::Info).await?;

  Ok(())
}

pub async fn disconnection_messages(state: &State) -> BluetoothResult<()> {
  state
    .client_man
    .broadcast(ServerBluetoothEvent::Status { connected: false }, ServerEventType::Info)
    .await?;
  state
    .client_man
    .broadcast(
      ServerBluetoothEvent::PairedDevices(state.get_devices().await),
      ServerEventType::Info,
    )
    .await?;

  state
    .client_man
    .broadcast_stock(StockConnectionSend::TransportStatus { payload: false })
    .await?;

  Ok(())
}
