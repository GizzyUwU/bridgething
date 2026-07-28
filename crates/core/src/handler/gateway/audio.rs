use libbridgething::{
  client::{
    AudioErrorReply as ClientAudioErrorReply, BridgeToClientAudioMsgEvent, TtsEnded as ClientTtsEnded,
    TtsStarted as ClientTtsStarted, VolumeChanged as ClientVolumeChanged,
  },
  gateway::{AudioErrorReply, GatewayToBridgeAudioMsgEventDispatch, TtsEnded, TtsStarted, VolumeChanged},
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

  async fn error_event(&self, params: AudioErrorReply) -> HandlerResult {
    tracing::warn!(error = ?params.error, "companion refused an audio verb");
    let event = BridgeToClientAudioMsgEvent::ErrorEvent(ClientAudioErrorReply { error: params.error });
    self.handle.state.bus.broadcast_event(event).await?;
    Ok(())
  }
}
