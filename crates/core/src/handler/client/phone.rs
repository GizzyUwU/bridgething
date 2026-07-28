use libbridgething::{
  AcceptCallAction, EndCallAction, InitiateCallType, PhoneCallService, PhoneError,
  client::{
    BridgeToClientPhoneMsg, BridgeToClientPhoneMsgEvent, ClientToBridgePhoneMsgDispatch, PhoneAcceptAction,
    PhoneCallAction, PhoneDtmfAction, PhoneEndAction, PhoneErrorReply, PhoneInitiateAction, PhoneMuteAction,
    PhoneStateReply,
  },
};

use super::{HandlerResult, MsgHandle};
use crate::state::TelephonyManager;

pub struct PhoneHandler {
  handle: MsgHandle,
}

impl PhoneHandler {
  pub fn new(handle: MsgHandle) -> Self {
    Self { handle }
  }

  async fn report(&self, error: PhoneError) {
    let event = BridgeToClientPhoneMsgEvent::ErrorEvent(PhoneErrorReply { error });
    if let Err(err) = self.handle.state.bus.send_event(self.handle.from, event).await {
      tracing::warn!(?err, "failed to report phone action failure to webapp");
    }
  }

  async fn known_call(&self, call_id: &str) -> bool {
    let known = self
      .handle
      .state
      .telephony
      .snapshot()
      .await
      .active_calls
      .iter()
      .any(|c| c.call_id == call_id);
    if !known {
      self
        .report(PhoneError::CallNotFound {
          call_id: call_id.to_string(),
        })
        .await;
    }
    known
  }

  async fn verb_available(
    &self,
    verb: &str,
    pick: impl Fn(&libbridgething::CommunicationsState) -> Option<bool>,
  ) -> bool {
    let comms = self.handle.state.telephony.communications().await;
    if pick(&comms) == Some(false) {
      self.report(PhoneError::Unavailable { verb: verb.to_string() }).await;
      return false;
    }
    true
  }
}

impl ClientToBridgePhoneMsgDispatch for PhoneHandler {
  type Output = HandlerResult;

  async fn answer(&self, params: PhoneCallAction) -> HandlerResult {
    if !self.known_call(&params.call_id).await {
      return Ok(());
    }
    let cmd = TelephonyManager::build_accept(0, Some(params.call_id));
    self.handle.state.telephony.dispatch(cmd).await.ok();
    Ok(())
  }

  async fn accept(&self, params: PhoneAcceptAction) -> HandlerResult {
    if !self.known_call(&params.call_id).await {
      return Ok(());
    }
    let action_byte = match params.action {
      AcceptCallAction::Accept => 0,
      AcceptCallAction::EndAndAccept => 1,
    };
    if action_byte == 1 && !self.verb_available("accept", |c| c.end_and_accept_available).await {
      return Ok(());
    }
    let cmd = TelephonyManager::build_accept(action_byte, Some(params.call_id));
    self.handle.state.telephony.dispatch(cmd).await.ok();
    Ok(())
  }

  async fn decline(&self, params: PhoneCallAction) -> HandlerResult {
    if !self.known_call(&params.call_id).await {
      return Ok(());
    }
    let cmd = TelephonyManager::build_end(0, Some(params.call_id));
    self.handle.state.telephony.dispatch(cmd).await.ok();
    Ok(())
  }

  async fn end(&self, params: PhoneCallAction) -> HandlerResult {
    if !self.known_call(&params.call_id).await {
      return Ok(());
    }
    let cmd = TelephonyManager::build_end(0, Some(params.call_id));
    self.handle.state.telephony.dispatch(cmd).await.ok();
    Ok(())
  }

  async fn end_typed(&self, params: PhoneEndAction) -> HandlerResult {
    if !self.known_call(&params.call_id).await {
      return Ok(());
    }
    let action_byte = match params.action {
      EndCallAction::End => 0,
      EndCallAction::EndAll => 1,
    };
    let cmd = TelephonyManager::build_end(action_byte, Some(params.call_id));
    self.handle.state.telephony.dispatch(cmd).await.ok();
    Ok(())
  }

  async fn hold(&self, params: PhoneCallAction) -> HandlerResult {
    if !self.known_call(&params.call_id).await || !self.verb_available("hold", |c| c.hold_available).await {
      return Ok(());
    }
    let cmd = TelephonyManager::build_hold(true, Some(params.call_id));
    self.handle.state.telephony.dispatch(cmd).await.ok();
    Ok(())
  }

  async fn unhold(&self, params: PhoneCallAction) -> HandlerResult {
    if !self.known_call(&params.call_id).await {
      return Ok(());
    }
    let cmd = TelephonyManager::build_hold(false, Some(params.call_id));
    self.handle.state.telephony.dispatch(cmd).await.ok();
    Ok(())
  }

  async fn initiate(&self, params: PhoneInitiateAction) -> HandlerResult {
    if !self.verb_available("initiate", |c| c.initiate_call_available).await {
      return Ok(());
    }
    let kind = match params.kind {
      InitiateCallType::Destination => 0,
      InitiateCallType::Voicemail => 1,
      InitiateCallType::Redial => 2,
    };
    let service = params.service.map(|s| match s {
      PhoneCallService::Telephony => 1,
      PhoneCallService::FaceTimeAudio => 2,
      PhoneCallService::FaceTimeVideo => 3,
      PhoneCallService::Unknown => 0,
    });
    let cmd = TelephonyManager::build_initiate(kind, params.destination_id, service, params.address_book_id);
    self.handle.state.telephony.dispatch(cmd).await.ok();
    Ok(())
  }

  async fn swap(&self) -> HandlerResult {
    if !self.verb_available("swap", |c| c.swap_available).await {
      return Ok(());
    }
    self
      .handle
      .state
      .telephony
      .dispatch(bridgething_iap2::session::TelephonyCommand::Swap)
      .await
      .ok();
    Ok(())
  }

  async fn merge(&self) -> HandlerResult {
    if !self.verb_available("merge", |c| c.merge_available).await {
      return Ok(());
    }
    self
      .handle
      .state
      .telephony
      .dispatch(bridgething_iap2::session::TelephonyCommand::Merge)
      .await
      .ok();
    Ok(())
  }

  async fn mute(&self, params: PhoneMuteAction) -> HandlerResult {
    let cmd = TelephonyManager::build_mute(params.mute);
    self.handle.state.telephony.dispatch(cmd).await.ok();
    Ok(())
  }

  async fn dtmf(&self, params: PhoneDtmfAction) -> HandlerResult {
    if let Some(id) = params.call_id.as_deref()
      && !self.known_call(id).await
    {
      return Ok(());
    }
    let cmd = TelephonyManager::build_dtmf(params.tone, params.call_id);
    self.handle.state.telephony.dispatch(cmd).await.ok();
    Ok(())
  }

  async fn state_get(&self) -> HandlerResult {
    let state = self.handle.state.telephony.snapshot().await;
    self
      .handle
      .respond(BridgeToClientPhoneMsg::StateReply(PhoneStateReply { state }))
      .await?;
    Ok(())
  }
}
