use libbridgething::client::ClientVoiceCommand;

use super::{HandlerResult, MsgHandle};

#[derive(Debug)]
pub struct VoiceHandler {
  handle: MsgHandle,
}

impl VoiceHandler {
  pub fn new(handle: MsgHandle) -> Self {
    Self { handle }
  }

  pub async fn handle(&self, msg: ClientVoiceCommand) -> HandlerResult {
    tracing::debug!("({}) handling voice message", &self.handle.from);

    match msg {
      ClientVoiceCommand::Cancel => self.cancel().await,
      ClientVoiceCommand::PushToTalk => self.push_to_talk().await,
      ClientVoiceCommand::MuteMic { preserve } => self.mute_mic(preserve).await,
      ClientVoiceCommand::UnmuteMic { preserve } => self.unmute_mic(preserve).await,
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
