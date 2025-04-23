use tokio_util::sync::CancellationToken;

use crate::{protocol::Protocol, Callback, Callbacks, MsgRx, Result};

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

pub async fn get_rfcomm(
  adapter_name: Option<String>,
  rx: MsgRx,
  callbacks: Callbacks,
  cancel_token: CancellationToken,
) -> Result<impl Protocol> {
  #[cfg(target_os = "linux")]
  let rfcomm = linux::Rfcomm::init(adapter_name, rx, callbacks, cancel_token).await;
  #[cfg(target_os = "macos")]
  let rfcomm = macos::Rfcomm::init(adapter_name, rx, callbacks, cancel_token).await;
  #[cfg(target_os = "windows")]
  let rfcomm = windows::Rfcomm::init(adapter_name, rx, callbacks, cancel_token).await;

  rfcomm
}
