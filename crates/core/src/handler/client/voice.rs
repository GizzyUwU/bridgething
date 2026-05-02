use libbridgething::client::{ClientToBridgeVoiceMsgCommand, MicMute, MicUnmute};

use super::{HandlerResult, MsgHandle};

#[derive(Debug)]
pub struct VoiceHandler {
  handle: MsgHandle,
}

impl VoiceHandler {
  pub fn new(handle: MsgHandle) -> Self {
    Self { handle }
  }

  pub async fn handle(&self, msg: ClientToBridgeVoiceMsgCommand) -> HandlerResult {
    tracing::debug!("({}) handling voice message", &self.handle.from);

    match msg {
      ClientToBridgeVoiceMsgCommand::Cancel => self.cancel().await,
      ClientToBridgeVoiceMsgCommand::PushToTalk => self.push_to_talk().await,
      ClientToBridgeVoiceMsgCommand::MuteMic(MicMute { preserve }) => self.mute_mic(preserve).await,
      ClientToBridgeVoiceMsgCommand::UnmuteMic(MicUnmute { preserve }) => self.unmute_mic(preserve).await,
    }
  }

  async fn cancel(&self) -> HandlerResult {
    tracing::debug!("({}) cancelling voice command", &self.handle.from);
    // Ok(self.handle.respond().await?)
    Ok(())
  }

  async fn push_to_talk(&self) -> HandlerResult {
    tracing::debug!("({}) activating push-to-talk", &self.handle.from);
    // Ok(self.handle.respond().await?)
    Ok(())
  }

  async fn mute_mic(&self, preserve: bool) -> HandlerResult {
    tracing::debug!("({}) muting microphone, preserve: {}", &self.handle.from, preserve);
    // Ok(self.handle.respond().await?)
    Ok(())
  }

  async fn unmute_mic(&self, preserve: bool) -> HandlerResult {
    tracing::debug!("({}) unmuting microphone, preserve: {}", &self.handle.from, preserve);
    // Ok(self.handle.respond().await?)
    Ok(())
  }
}
