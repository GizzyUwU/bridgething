use libbridgething::{
  client::{ClientToBridgeAudioMsgCommandDispatch, Earcon, SetMute, SetVolume, Tts, TtsCancel},
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

  async fn forward(&self, cmd: BridgeToGatewayAudioMsgCommand) -> HandlerResult {
    self.handle.bluetooth.gateway_man.broadcast_command(cmd).await;
    Ok(())
  }

  async fn actuate_volume(&self) -> HandlerResult {
    self.handle.state.audio.broadcast_current().await?;
    Ok(())
  }
}

impl ClientToBridgeAudioMsgCommandDispatch for AudioHandler {
  type Output = HandlerResult;

  async fn volume_up(&self) -> HandlerResult {
    let transport = self.handle.transport.clone();
    transport.volume_up().await;
    self.actuate_volume().await
  }

  async fn volume_down(&self) -> HandlerResult {
    let transport = self.handle.transport.clone();
    transport.volume_down().await;
    self.actuate_volume().await
  }

  async fn set_volume(&self, params: SetVolume) -> HandlerResult {
    self
      .handle
      .bluetooth
      .gateway_man
      .broadcast_command(BridgeToGatewayAudioMsgCommand::SetVolume(gateway::SetVolume {
        level: params.level,
      }))
      .await;
    self.handle.state.audio.broadcast_current().await?;
    Ok(())
  }

  async fn mute_toggle(&self) -> HandlerResult {
    let transport = self.handle.transport.clone();
    transport.mute_toggle().await;
    self.actuate_volume().await
  }

  async fn set_mute(&self, params: SetMute) -> HandlerResult {
    self
      .handle
      .bluetooth
      .gateway_man
      .broadcast_command(BridgeToGatewayAudioMsgCommand::SetMute(gateway::SetMute {
        muted: params.muted,
      }))
      .await;
    self.handle.state.audio.broadcast_current().await?;
    Ok(())
  }

  async fn tts(&self, params: Tts) -> HandlerResult {
    self
      .forward(BridgeToGatewayAudioMsgCommand::Tts(gateway::Tts {
        id: params.id,
        text: params.text,
        voice: params.voice,
      }))
      .await
  }

  async fn tts_cancel(&self, params: TtsCancel) -> HandlerResult {
    self
      .forward(BridgeToGatewayAudioMsgCommand::TtsCancel(gateway::TtsCancel {
        id: params.id,
      }))
      .await
  }

  async fn tts_cancel_all(&self) -> HandlerResult {
    self.forward(BridgeToGatewayAudioMsgCommand::TtsCancelAll).await
  }

  async fn earcon(&self, params: Earcon) -> HandlerResult {
    self
      .forward(BridgeToGatewayAudioMsgCommand::Earcon(gateway::Earcon {
        name: params.name,
      }))
      .await
  }
}
