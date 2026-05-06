use libbridgething::{
  client::{
    BridgeToClientAudioMsgEvent, TtsEnded as ClientTtsEnded, TtsStarted as ClientTtsStarted,
    VolumeChanged as ClientVolumeChanged,
  },
  gateway::GatewayToBridgeAudioMsgEvent,
};

use super::{HandlerResult, MsgHandle};

pub struct AudioHandler {
  handle: MsgHandle,
}

impl AudioHandler {
  pub fn new(handle: MsgHandle) -> Self {
    Self { handle }
  }

  pub async fn handle(self, msg: GatewayToBridgeAudioMsgEvent) -> HandlerResult {
    match msg {
      GatewayToBridgeAudioMsgEvent::TtsStarted(started) => {
        let event = BridgeToClientAudioMsgEvent::TtsStarted(ClientTtsStarted { id: started.id });
        self.handle.state.bus.broadcast_event(event).await?;
      }
      GatewayToBridgeAudioMsgEvent::TtsEnded(ended) => {
        let event = BridgeToClientAudioMsgEvent::TtsEnded(ClientTtsEnded {
          id: ended.id,
          completed: ended.completed,
        });
        self.handle.state.bus.broadcast_event(event).await?;
      }
      GatewayToBridgeAudioMsgEvent::VolumeChanged(vol) => {
        self
          .handle
          .state
          .audio
          .apply_companion(ClientVolumeChanged {
            level: vol.level,
            muted: vol.muted,
          })
          .await?;
      }
    }
    Ok(())
  }
}
