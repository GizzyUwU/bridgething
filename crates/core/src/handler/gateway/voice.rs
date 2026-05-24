use libbridgething::{
  VoiceDispatchErrorCode,
  gateway::{
    BridgeToGatewayVoiceMsg, GatewayToBridgeVoiceMsgCommandDispatch, VoiceCloseReason, VoiceDispatch,
    VoiceDispatchFailed, VoiceMicOpen,
  },
};

use super::{HandlerResult, MsgHandle};

pub struct VoiceHandler {
  handle: MsgHandle,
}

impl VoiceHandler {
  pub fn new(handle: MsgHandle) -> Self {
    Self { handle }
  }
}

impl GatewayToBridgeVoiceMsgCommandDispatch for VoiceHandler {
  type Output = HandlerResult;

  async fn mic_open(&self, _params: VoiceMicOpen) -> HandlerResult {
    match self.handle.state.mic.push_to_talk().await {
      Ok(stream_id) => tracing::debug!("({:?}) gateway opened mic -> stream {stream_id}", &self.handle.address),
      Err(err) => tracing::warn!("({:?}) gateway mic open failed: {err}", &self.handle.address),
    }
    Ok(())
  }

  async fn mic_close(&self) -> HandlerResult {
    if let Err(err) = self.handle.state.mic.stop_with(VoiceCloseReason::Cancelled).await {
      tracing::warn!("({:?}) gateway mic close failed: {err}", &self.handle.address);
    }
    Ok(())
  }

  async fn dispatch(&self, params: VoiceDispatch) -> HandlerResult {
    let VoiceDispatch { resolved } = params;
    tracing::info!(
      "({:?}) voice dispatch received: intent={} transcript={:?} webapp_id={:?}",
      &self.handle.address,
      resolved.intent,
      resolved.transcript,
      resolved.slots.webapp_id,
    );
    self
      .handle
      .send_info(BridgeToGatewayVoiceMsg::DispatchFailed(VoiceDispatchFailed {
        code: VoiceDispatchErrorCode::Internal,
        intent: resolved.intent,
        webapp_id: resolved.slots.webapp_id,
        msg: "dispatch routing pending hardware bring-up; see todo.md voice section".into(),
      }))
      .await;
    Ok(())
  }
}
