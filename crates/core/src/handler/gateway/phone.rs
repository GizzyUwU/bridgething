use libbridgething::{
  PhoneCall,
  client::{BridgeToClientPhoneMsgEvent, PhoneErrorReply as ClientPhoneErrorReply},
  gateway::{
    CommunicationsSnapshot, GatewayToBridgePhoneMsgEventDispatch, PhoneCallEnded, PhoneErrorReply, PhoneStateReply,
  },
};

use super::{HandlerResult, MsgHandle};

pub struct PhoneHandler {
  handle: MsgHandle,
}

impl PhoneHandler {
  pub fn new(handle: MsgHandle) -> Self {
    Self { handle }
  }
}

impl GatewayToBridgePhoneMsgEventDispatch for PhoneHandler {
  type Output = HandlerResult;

  async fn snapshot(&self, params: PhoneStateReply) -> HandlerResult {
    if let Err(err) = self.handle.state.telephony.apply_companion_snapshot(params.state).await {
      tracing::warn!(?err, "failed to apply companion phone event");
    }
    Ok(())
  }

  async fn communications_snapshot(&self, params: CommunicationsSnapshot) -> HandlerResult {
    if let Err(err) = self
      .handle
      .state
      .telephony
      .apply_companion_communications(params.state)
      .await
    {
      tracing::warn!(?err, "failed to apply companion phone event");
    }
    Ok(())
  }

  async fn call_started(&self, params: PhoneCall) -> HandlerResult {
    if let Err(err) = self.handle.state.telephony.apply_companion_call_started(params).await {
      tracing::warn!(?err, "failed to apply companion phone event");
    }
    Ok(())
  }

  async fn call_updated(&self, params: PhoneCall) -> HandlerResult {
    if let Err(err) = self.handle.state.telephony.apply_companion_call_updated(params).await {
      tracing::warn!(?err, "failed to apply companion phone event");
    }
    Ok(())
  }

  async fn call_ended(&self, params: PhoneCallEnded) -> HandlerResult {
    if let Err(err) = self
      .handle
      .state
      .telephony
      .apply_companion_call_ended(params.call_id, params.reason)
      .await
    {
      tracing::warn!(?err, "failed to apply companion phone event");
    }
    Ok(())
  }

  async fn error_event(&self, params: PhoneErrorReply) -> HandlerResult {
    tracing::warn!(error = ?params.error, "companion refused a phone action");
    self
      .handle
      .state
      .bus
      .broadcast_event(BridgeToClientPhoneMsgEvent::ErrorEvent(ClientPhoneErrorReply {
        error: params.error,
      }))
      .await?;
    Ok(())
  }
}
