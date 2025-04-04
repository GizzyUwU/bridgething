use std::time::Duration;

use bluer::Device;

pub mod art;

pub const AVRCP_UUID: bluer::Uuid = bluer::Uuid::from_u128(0x110c00001000800000805f9b34fb);

pub async fn connect_avrcp(device: &Device) -> bool {
  loop {
    tracing::debug!("attempting to connect to avrcp profile...");
    match device.connect_profile(&AVRCP_UUID).await {
      Ok(()) => {
        tracing::info!("avrcp profile connected!");
        return true;
      }
      Err(err) => {
        tracing::debug!("failed to connect to avrcp profile: {:?}", err);
        tokio::time::sleep(Duration::from_secs(2)).await;
      }
    };
  }
}
