use libbridgething::{
  client::{
    BridgeToClientAudioMsgEvent, TtsEnded as ClientTtsEnded, TtsStarted as ClientTtsStarted,
    VolumeChanged as ClientVolumeChanged,
  },
  gateway::{GatewayToBridgeAudioMsgEventDispatch, TtsEnded, TtsStarted, VolumeChanged},
};

use super::{HandlerResult, MsgHandle};

pub struct AudioHandler {
  handle: MsgHandle,
}

impl AudioHandler {
  pub fn new(handle: MsgHandle) -> Self {
    Self { handle }
  }
}

impl GatewayToBridgeAudioMsgEventDispatch for AudioHandler {
  type Output = HandlerResult;

  async fn tts_started(&self, params: TtsStarted) -> HandlerResult {
    let event = BridgeToClientAudioMsgEvent::TtsStarted(ClientTtsStarted { id: params.id });
    self.handle.state.bus.broadcast_event(event).await?;
    Ok(())
  }

  async fn tts_ended(&self, params: TtsEnded) -> HandlerResult {
    let event = BridgeToClientAudioMsgEvent::TtsEnded(ClientTtsEnded {
      id: params.id,
      completed: params.completed,
    });
    self.handle.state.bus.broadcast_event(event).await?;
    Ok(())
  }

  async fn volume_changed(&self, params: VolumeChanged) -> HandlerResult {
    self
      .handle
      .state
      .audio
      .apply_companion(ClientVolumeChanged {
        level: params.level,
        muted: params.muted,
      })
      .await?;
    Ok(())
  }
}
