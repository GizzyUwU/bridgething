use libbridgething::gateway::{GatewayToBridgeVoiceMsgCommand, VoiceCloseReason};

use super::{HandlerResult, MsgHandle};

pub struct VoiceHandler {
  handle: MsgHandle,
}

impl VoiceHandler {
  pub fn new(handle: MsgHandle) -> Self {
    Self { handle }
  }

  pub async fn handle(self, cmd: GatewayToBridgeVoiceMsgCommand) -> HandlerResult {
    match cmd {
      GatewayToBridgeVoiceMsgCommand::MicOpen(_) => match self.handle.state.mic.push_to_talk().await {
        Ok(stream_id) => tracing::debug!("({:?}) gateway opened mic -> stream {stream_id}", &self.handle.address),
        Err(err) => tracing::warn!("({:?}) gateway mic open failed: {err}", &self.handle.address),
      },
      GatewayToBridgeVoiceMsgCommand::MicClose => {
        if let Err(err) = self.handle.state.mic.stop_with(VoiceCloseReason::Cancelled).await {
          tracing::warn!("({:?}) gateway mic close failed: {err}", &self.handle.address);
        }
      }
    }
    Ok(())
  }
}
