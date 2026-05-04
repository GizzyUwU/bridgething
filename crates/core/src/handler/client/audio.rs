use libbridgething::{
  client::ClientToBridgeAudioMsgCommand,
  gateway::{self, BridgeToGatewayAudioMsgCommand},
};

use super::{HandlerResult, MsgHandle};

pub struct AudioHandler {
  handle: MsgHandle,
}

impl AudioHandler {
  pub fn new(handle: MsgHandle) -> Self {
    Self { handle }
  }

  pub async fn handle(self, msg: ClientToBridgeAudioMsgCommand) -> HandlerResult {
    let transport = self.handle.transport.clone();
    match msg {
      ClientToBridgeAudioMsgCommand::VolumeUp => Ok(transport.volume_up().await?),
      ClientToBridgeAudioMsgCommand::VolumeDown => Ok(transport.volume_down().await?),
      ClientToBridgeAudioMsgCommand::SetVolume(req) => {
        self
          .forward(BridgeToGatewayAudioMsgCommand::SetVolume(gateway::SetVolume {
            level: req.level,
          }))
          .await
      }
      ClientToBridgeAudioMsgCommand::MuteToggle => Ok(transport.mute_toggle().await?),
      ClientToBridgeAudioMsgCommand::SetMute(req) => {
        self
          .forward(BridgeToGatewayAudioMsgCommand::SetMute(gateway::SetMute {
            muted: req.muted,
          }))
          .await
      }
      ClientToBridgeAudioMsgCommand::Tts(req) => {
        self
          .forward(BridgeToGatewayAudioMsgCommand::Tts(gateway::Tts {
            id: req.id,
            text: req.text,
            voice: req.voice,
          }))
          .await
      }
      ClientToBridgeAudioMsgCommand::TtsCancel(req) => {
        self
          .forward(BridgeToGatewayAudioMsgCommand::TtsCancel(gateway::TtsCancel {
            id: req.id,
          }))
          .await
      }
      ClientToBridgeAudioMsgCommand::TtsCancelAll => self.forward(BridgeToGatewayAudioMsgCommand::TtsCancelAll).await,
      ClientToBridgeAudioMsgCommand::Earcon(req) => {
        self
          .forward(BridgeToGatewayAudioMsgCommand::Earcon(gateway::Earcon {
            name: req.name,
          }))
          .await
      }
    }
  }

  async fn forward(self, cmd: BridgeToGatewayAudioMsgCommand) -> HandlerResult {
    self.handle.bluetooth.gateway_man.broadcast_command(cmd).await;
    Ok(())
  }
}
