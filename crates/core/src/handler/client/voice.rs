use libbridgething::client::{BridgeToClientVoiceMsg, ClientToBridgeVoiceMsg, MicMute, MicUnmute, VoiceStateReply};

use super::{HandlerResult, MsgHandle};

#[derive(Debug)]
pub struct VoiceHandler {
  handle: MsgHandle,
}

impl VoiceHandler {
  pub fn new(handle: MsgHandle) -> Self {
    Self { handle }
  }

  pub async fn handle(&self, msg: ClientToBridgeVoiceMsg) -> HandlerResult {
    match msg {
      ClientToBridgeVoiceMsg::Cancel => {
        if let Err(err) = self.handle.state.mic.cancel().await {
          tracing::warn!("({}) voice.cancel failed: {err}", &self.handle.from);
        }
      }
      ClientToBridgeVoiceMsg::PushToTalk => match self.handle.state.mic.push_to_talk().await {
        Ok(stream_id) => tracing::debug!("({}) voice.pushToTalk -> stream {stream_id}", &self.handle.from),
        Err(err) => tracing::warn!("({}) voice.pushToTalk failed: {err}", &self.handle.from),
      },
      ClientToBridgeVoiceMsg::MuteMic(MicMute { preserve }) => {
        if let Err(err) = self.handle.state.mic.set_muted(true, preserve).await {
          tracing::warn!("({}) voice.muteMic failed: {err}", &self.handle.from);
        }
      }
      ClientToBridgeVoiceMsg::UnmuteMic(MicUnmute { preserve }) => {
        if let Err(err) = self.handle.state.mic.set_muted(false, preserve).await {
          tracing::warn!("({}) voice.unmuteMic failed: {err}", &self.handle.from);
        }
      }
      ClientToBridgeVoiceMsg::StateGet => {
        let state = self.handle.state.mic.snapshot().await;
        self
          .handle
          .respond(BridgeToClientVoiceMsg::StateReply(VoiceStateReply { state }))
          .await?;
      }
    }
    Ok(())
  }
}
