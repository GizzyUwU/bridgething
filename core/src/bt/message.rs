use crate::{
  msg::{
    stock::{StockConfigurationSend, StockConnectionSend, StockInterAppSend, StockInterAppSendPayload, StockSetupSend},
    BluetoothSend, Device, SendMsgMeta,
  },
  state::State,
  ws::ConnMan,
};

use super::BluetoothResult;

pub async fn connection_messages(
  conn_man: &mut ConnMan,
  state: &State,
  new_device: bool,
  device: &Device,
) -> BluetoothResult<()> {
  if new_device {
    conn_man
      .broadcast(BluetoothSend::ParingResult { success: true }, SendMsgMeta::Info)
      .await?;
  }

  conn_man
    .broadcast(BluetoothSend::Status { connected: true }, SendMsgMeta::Info)
    .await?;
  conn_man
    .broadcast(
      BluetoothSend::PairedDevices(state.get_devices().clone()),
      SendMsgMeta::Info,
    )
    .await?;
  conn_man
    .broadcast(
      BluetoothSend::ConnectedDevice {
        name: device.name.clone(),
        mac: device.mac.clone(),
      },
      SendMsgMeta::Info,
    )
    .await?;

  conn_man
    .broadcast_stock(StockConnectionSend::RemoteStatus {
      payload: true,
      mac: device.mac.clone(),
      phone_type: device.device_type.clone().into(),
    })
    .await?;
  conn_man
    .broadcast_stock(StockConnectionSend::TransportStatus { payload: true })
    .await?;
  conn_man.broadcast_stock(StockConfigurationSend::default()).await?;

  if new_device {
    conn_man
      .broadcast_stock(StockSetupSend::Status {
        payload: "finished".to_string(),
      })
      .await?;
  }

  conn_man
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
  conn_man
    .broadcast_stock(StockConnectionSend::RemoteApp {
      app_id: "com.bridgething".to_string(),
      is_spotify: true,
    })
    .await?;

  if let Some(player) = &state.player {
    player.send_state(conn_man).await?;
  }

  // TODO: remove testing code
  // #[cfg(debug_assertions)]
  // conn_man.broadcast(PlayerSend::dummy(), SendMsgMeta::Info).await?;
  // #[cfg(debug_assertions)]
  // conn_man.broadcast(PlayerSend::dummy_queue(), SendMsgMeta::Info).await?;

  Ok(())
}

pub async fn disconnection_messages(conn_man: &mut ConnMan, state: &State) -> BluetoothResult<()> {
  conn_man
    .broadcast(BluetoothSend::Status { connected: false }, SendMsgMeta::Info)
    .await?;
  conn_man
    .broadcast(
      BluetoothSend::PairedDevices(state.get_devices().clone()),
      SendMsgMeta::Info,
    )
    .await?;

  conn_man
    .broadcast_stock(StockConnectionSend::TransportStatus { payload: false })
    .await?;

  Ok(())
}
