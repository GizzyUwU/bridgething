use crate::{msg::VoiceRecv, state::State};

use super::{Handler, HandlerResult, MsgHandle};

#[derive(Debug)]
pub struct VoiceHandler<'a> {
  handle: MsgHandle<'a>,
  state: &'a mut State,
}

impl<'a> VoiceHandler<'a> {
  pub fn new(handler: Handler<'a>) -> Self {
    Self {
      handle: handler.handle,
      state: handler.state,
    }
  }

  pub async fn handle(&self, msg: VoiceRecv) -> HandlerResult {
    tracing::debug!("({}) handling voice message", &self.handle.id);

    match msg {
      VoiceRecv::Cancel => self.cancel().await,
      VoiceRecv::PushToTalk => self.push_to_talk().await,
      VoiceRecv::MuteMic { preserve } => self.mute_mic(preserve).await,
      VoiceRecv::UnmuteMic { preserve } => self.unmute_mic(preserve).await,
    }
  }

  async fn cancel(&self) -> HandlerResult {
    tracing::debug!("({}) cancelling voice command", &self.handle.id);
    // Ok(self.handle.respond().await?)
    Ok(())
  }

  async fn push_to_talk(&self) -> HandlerResult {
    tracing::debug!("({}) activating push-to-talk", &self.handle.id);
    // Ok(self.handle.respond().await?)
    Ok(())
  }

  async fn mute_mic(&self, preserve: bool) -> HandlerResult {
    tracing::debug!("({}) muting microphone, preserve: {}", &self.handle.id, preserve);
    // Ok(self.handle.respond().await?)
    Ok(())
  }

  async fn unmute_mic(&self, preserve: bool) -> HandlerResult {
    tracing::debug!("({}) unmuting microphone, preserve: {}", &self.handle.id, preserve);
    // Ok(self.handle.respond().await?)
    Ok(())
  }
}
