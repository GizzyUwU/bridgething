use libbridgething::{
  AcceptCallAction, EndCallAction, InitiateCallType, PhoneCallService,
  client::{
    BridgeToClientPhoneMsg, ClientToBridgePhoneMsgDispatch, PhoneAcceptAction, PhoneCallAction, PhoneDtmfAction,
    PhoneEndAction, PhoneInitiateAction, PhoneMuteAction, PhoneStateReply,
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
}

impl ClientToBridgePhoneMsgDispatch for PhoneHandler {
  type Output = HandlerResult;

  async fn answer(&self, params: PhoneCallAction) -> HandlerResult {
    let cmd = TelephonyManager::build_accept(0, Some(params.call_id));
    self.handle.state.telephony.dispatch(cmd).await.ok();
    Ok(())
  }

  async fn accept(&self, params: PhoneAcceptAction) -> HandlerResult {
    let action_byte = match params.action {
      AcceptCallAction::Accept => 0,
      AcceptCallAction::EndAndAccept => 1,
    };
    let cmd = TelephonyManager::build_accept(action_byte, Some(params.call_id));
    self.handle.state.telephony.dispatch(cmd).await.ok();
    Ok(())
  }

  async fn decline(&self, params: PhoneCallAction) -> HandlerResult {
    let cmd = TelephonyManager::build_end(0, Some(params.call_id));
    self.handle.state.telephony.dispatch(cmd).await.ok();
    Ok(())
  }

  async fn end(&self, params: PhoneCallAction) -> HandlerResult {
    let cmd = TelephonyManager::build_end(0, Some(params.call_id));
    self.handle.state.telephony.dispatch(cmd).await.ok();
    Ok(())
  }

  async fn end_typed(&self, params: PhoneEndAction) -> HandlerResult {
    let action_byte = match params.action {
      EndCallAction::End => 0,
      EndCallAction::EndAll => 1,
    };
    let cmd = TelephonyManager::build_end(action_byte, Some(params.call_id));
    self.handle.state.telephony.dispatch(cmd).await.ok();
    Ok(())
  }

  async fn hold(&self, params: PhoneCallAction) -> HandlerResult {
    let cmd = TelephonyManager::build_hold(true, Some(params.call_id));
    self.handle.state.telephony.dispatch(cmd).await.ok();
    Ok(())
  }

  async fn unhold(&self, params: PhoneCallAction) -> HandlerResult {
    let cmd = TelephonyManager::build_hold(false, Some(params.call_id));
    self.handle.state.telephony.dispatch(cmd).await.ok();
    Ok(())
  }

  async fn initiate(&self, params: PhoneInitiateAction) -> HandlerResult {
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
