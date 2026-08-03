use libbridgething::{
  client::{BridgeToClientVoiceMsg, ClientToBridgeVoiceMsgDispatch, MicMute, MicUnmute, VoiceStateReply},
  gateway::VoiceCloseReason,
};

use super::{HandlerResult, MsgHandle};

#[derive(Debug)]
pub struct VoiceHandler {
  handle: MsgHandle,
}

impl VoiceHandler {
  pub fn new(handle: MsgHandle) -> Self {
    Self { handle }
  }
}

impl ClientToBridgeVoiceMsgDispatch for VoiceHandler {
  type Output = HandlerResult;

  async fn cancel(&self) -> HandlerResult {
    if let Err(err) = self.handle.state.mic.cancel().await {
      tracing::warn!("({}) voice.cancel failed: {err}", &self.handle.from);
    }
    Ok(())
  }

  async fn push_to_talk(&self) -> HandlerResult {
    match self.handle.state.mic.push_to_talk().await {
      Ok(stream_id) => tracing::debug!("({}) voice.pushToTalk -> stream {stream_id}", &self.handle.from),
      Err(err) => tracing::warn!("({}) voice.pushToTalk failed: {err}", &self.handle.from),
    }
    Ok(())
  }

  async fn release(&self) -> HandlerResult {
    if let Err(err) = self.handle.state.mic.stop_with(VoiceCloseReason::EndOfSpeech).await {
      tracing::warn!("({}) voice.release failed: {err}", &self.handle.from);
    }
    Ok(())
  }

  async fn mute_mic(&self, params: MicMute) -> HandlerResult {
    if let Err(err) = self.handle.state.mic.set_muted(true, params.preserve).await {
      tracing::warn!("({}) voice.muteMic failed: {err}", &self.handle.from);
    }
    Ok(())
  }

  async fn unmute_mic(&self, params: MicUnmute) -> HandlerResult {
    if let Err(err) = self.handle.state.mic.set_muted(false, params.preserve).await {
      tracing::warn!("({}) voice.unmuteMic failed: {err}", &self.handle.from);
    }
    Ok(())
  }

  async fn state_get(&self) -> HandlerResult {
    let state = self.handle.state.mic.snapshot().await;
    self
      .handle
      .respond(BridgeToClientVoiceMsg::StateReply(VoiceStateReply { state }))
      .await?;
    Ok(())
  }
}
