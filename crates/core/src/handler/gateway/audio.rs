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
    let event = match msg {
      GatewayToBridgeAudioMsgEvent::TtsStarted(started) => {
        BridgeToClientAudioMsgEvent::TtsStarted(ClientTtsStarted { id: started.id })
      }
      GatewayToBridgeAudioMsgEvent::TtsEnded(ended) => BridgeToClientAudioMsgEvent::TtsEnded(ClientTtsEnded {
        id: ended.id,
        completed: ended.completed,
      }),
      GatewayToBridgeAudioMsgEvent::VolumeChanged(vol) => {
        BridgeToClientAudioMsgEvent::VolumeChanged(ClientVolumeChanged {
          level: vol.level,
          muted: vol.muted,
        })
      }
    };
    self.handle.state.bus.broadcast_event(event).await?;
    Ok(())
  }
}
