use libbridgething::gateway::GatewayToBridgeAudioMsg;

use super::{HandlerResult, MsgHandle};

pub struct AudioHandler {
  handle: MsgHandle,
}

impl AudioHandler {
  pub fn new(handle: MsgHandle) -> Self {
    Self { handle }
  }

  pub async fn handle(self, msg: GatewayToBridgeAudioMsg) -> HandlerResult {
    match msg {
      GatewayToBridgeAudioMsg::TtsStarted(_) => self.handle.unimplemented("gateway:audio.ttsStarted").await,
      GatewayToBridgeAudioMsg::TtsEnded(_) => self.handle.unimplemented("gateway:audio.ttsEnded").await,
      GatewayToBridgeAudioMsg::VolumeChanged(_) => self.handle.unimplemented("gateway:audio.volumeChanged").await,
    }
    Ok(())
  }
}
