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
      ClientToBridgeAudioMsgCommand::VolumeUp => self.actuate_volume(transport.volume_up().await).await,
      ClientToBridgeAudioMsgCommand::VolumeDown => self.actuate_volume(transport.volume_down().await).await,
      ClientToBridgeAudioMsgCommand::SetVolume(req) => {
        self
          .handle
          .bluetooth
          .gateway_man
          .broadcast_command(BridgeToGatewayAudioMsgCommand::SetVolume(gateway::SetVolume {
            level: req.level,
          }))
          .await;
        self.handle.state.audio.broadcast_current().await?;
        Ok(())
      }
      ClientToBridgeAudioMsgCommand::MuteToggle => self.actuate_volume(transport.mute_toggle().await).await,
      ClientToBridgeAudioMsgCommand::SetMute(req) => {
        self
          .handle
          .bluetooth
          .gateway_man
          .broadcast_command(BridgeToGatewayAudioMsgCommand::SetMute(gateway::SetMute {
            muted: req.muted,
          }))
          .await;
        self.handle.state.audio.broadcast_current().await?;
        Ok(())
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

  /// Volume / mute commands always re-broadcast the current state so
  /// the kiosk receives a `volume_state` push in response to its
  /// `volume_up/down/mute` send. When a companion holds Volume
  /// authority that broadcast carries the companion-reported value;
  /// otherwise it carries the placeholder, so the slider never flags a
  /// mute condition. The `transport` actuation is best-effort — a
  /// `NoTarget` error (no iAP2 link) is swallowed so the UI still
  /// updates.
  async fn actuate_volume(self, actuate: Result<(), crate::transport::TransportError>) -> HandlerResult {
    if let Err(err) = actuate {
      tracing::debug!(?err, "audio command actuation dropped (no transport target)");
    }
    self.handle.state.audio.broadcast_current().await?;
    Ok(())
  }
}
