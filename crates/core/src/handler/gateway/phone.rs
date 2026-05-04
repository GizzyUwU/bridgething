use libbridgething::gateway::GatewayToBridgePhoneMsgEvent;

use super::{HandlerResult, MsgHandle};

pub struct PhoneHandler {
  handle: MsgHandle,
}

impl PhoneHandler {
  pub fn new(handle: MsgHandle) -> Self {
    Self { handle }
  }

  pub async fn handle(self, msg: GatewayToBridgePhoneMsgEvent) -> HandlerResult {
    let telephony = self.handle.state.telephony.clone();
    let result = match msg {
      GatewayToBridgePhoneMsgEvent::Snapshot(snapshot) => telephony.apply_companion_snapshot(snapshot.state).await,
      GatewayToBridgePhoneMsgEvent::CommunicationsSnapshot(snapshot) => {
        telephony.apply_companion_communications(snapshot.state).await
      }
      GatewayToBridgePhoneMsgEvent::CallStarted(call) => telephony.apply_companion_call_started(call).await,
      GatewayToBridgePhoneMsgEvent::CallUpdated(call) => telephony.apply_companion_call_updated(call).await,
      GatewayToBridgePhoneMsgEvent::CallEnded(ended) => {
        telephony.apply_companion_call_ended(ended.call_id, ended.reason).await
      }
    };
    if let Err(err) = result {
      tracing::warn!(?err, "failed to apply companion phone event");
    }
    Ok(())
  }
}
