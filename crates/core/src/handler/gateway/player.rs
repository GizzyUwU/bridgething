use std::time::Duration;

use bridgething_iap2::HidCommand;
use libbridgething::{
  PlayerState,
  gateway::{GatewayToBridgePlayerMsgCommandDispatch, GatewayToBridgePlayerMsgEventDispatch, QueueSnapshot},
};

use super::{HandlerResult, MsgHandle};
use crate::{bluetooth::iap2::SPOTIFY_BUNDLE_ID, transport::hid_bit};

const WAKE_LAUNCH_SETTLE: Duration = Duration::from_millis(1500);

pub struct PlayerHandler {
  handle: MsgHandle,
}

impl PlayerHandler {
  pub fn new(handle: MsgHandle) -> Self {
    Self { handle }
  }
}

impl GatewayToBridgePlayerMsgEventDispatch for PlayerHandler {
  type Output = HandlerResult;

  async fn snapshot(&self, params: PlayerState) -> HandlerResult {
    self.handle.state.player.apply_companion_snapshot(params).await?;
    Ok(())
  }

  async fn queue_changed(&self, params: QueueSnapshot) -> HandlerResult {
    self.handle.state.player.apply_companion_queue(params).await?;
    Ok(())
  }
}

impl GatewayToBridgePlayerMsgCommandDispatch for PlayerHandler {
  type Output = HandlerResult;

  async fn request_spotify_wake(&self) -> HandlerResult {
    let transport = &self.handle.bluetooth.iap2.transport;
    let bundle = self.handle.state.player.iap2_app_bundle();
    let playing = self.handle.state.player.iap2_playing().unwrap_or(false);
    match bundle.as_deref() {
      Some(SPOTIFY_BUNDLE_ID) if playing => {
        tracing::debug!("spotify wake requested but spotify is already playing; ignoring");
      }
      Some(SPOTIFY_BUNDLE_ID) => {
        tracing::info!("spotify wake: spotify owns now-playing but is paused; tapping play");
        transport.send_hid(HidCommand::Pulse(hid_bit::PLAY_PAUSE)).await;
      }
      Some(other) => {
        tracing::info!(bundle = %other, "spotify wake requested while another app owns now-playing; ignoring");
      }
      None => {
        tracing::info!("spotify wake: nothing playing; launching spotify then tapping play");
        transport.wake_spotify().await;
        tokio::time::sleep(WAKE_LAUNCH_SETTLE).await;
        transport.send_hid(HidCommand::Pulse(hid_bit::PLAY_PAUSE)).await;
      }
    }
    Ok(())
  }
}
