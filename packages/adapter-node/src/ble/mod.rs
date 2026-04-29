use crate::{Result, protocol::Protocol};

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

pub async fn get_ble(
  adapter_name: Option<String>,
  rx: MsgRx,
  callbacks: Vec<Callback>,
  cancel_token: CancellationToken,
) -> Result<Box<dyn Protocol>> {
  #[cfg(target_os = "linux")]
  let ble = linux::Ble::init(adapter_name, rx, callbacks, cancel_token).await;
  #[cfg(target_os = "macos")]
  let ble = macos::Ble::init(adapter_name, rx, callbacks, cancel_token).await;
  #[cfg(target_os = "windows")]
  let ble = windows::Ble::init(adapter_name, rx, callbacks, cancel_token).await;

  Ok(ble?)
}
