use libbridgething::client::ClientToBridgeAudioMsgCommand;

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
      ClientToBridgeAudioMsgCommand::SetVolume(_) => Ok(self.handle.unimplemented("audio.setVolume").await?),
      ClientToBridgeAudioMsgCommand::MuteToggle => Ok(transport.mute_toggle().await?),
      ClientToBridgeAudioMsgCommand::SetMute(_) => Ok(self.handle.unimplemented("audio.setMute").await?),
      ClientToBridgeAudioMsgCommand::Tts(_) => Ok(self.handle.unimplemented("audio.tts").await?),
      ClientToBridgeAudioMsgCommand::TtsCancel(_) => Ok(self.handle.unimplemented("audio.ttsCancel").await?),
      ClientToBridgeAudioMsgCommand::TtsCancelAll => Ok(self.handle.unimplemented("audio.ttsCancelAll").await?),
      ClientToBridgeAudioMsgCommand::Earcon(_) => Ok(self.handle.unimplemented("audio.earcon").await?),
    }
  }
}
