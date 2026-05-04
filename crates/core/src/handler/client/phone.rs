use libbridgething::{
  AcceptCallAction, EndCallAction, InitiateCallType, PhoneCallService,
  client::{BridgeToClientPhoneMsg, ClientToBridgePhoneMsg, PhoneStateReply},
};

use super::{HandlerResult, MsgHandle};
use crate::telephony::TelephonyManager;

pub struct PhoneHandler {
  handle: MsgHandle,
}

impl PhoneHandler {
  pub fn new(handle: MsgHandle) -> Self {
    Self { handle }
  }

  pub async fn handle(self, msg: ClientToBridgePhoneMsg) -> HandlerResult {
    let telephony = self.handle.state.telephony.clone();
    match msg {
      ClientToBridgePhoneMsg::Answer(action) => {
        let cmd = TelephonyManager::build_accept(0, Some(action.call_id));
        telephony.dispatch(cmd).await.ok();
      }
      ClientToBridgePhoneMsg::Accept(action) => {
        let action_byte = match action.action {
          AcceptCallAction::Accept => 0,
          AcceptCallAction::EndAndAccept => 1,
        };
        let cmd = TelephonyManager::build_accept(action_byte, Some(action.call_id));
        telephony.dispatch(cmd).await.ok();
      }
      ClientToBridgePhoneMsg::Decline(action) => {
        let cmd = TelephonyManager::build_end(0, Some(action.call_id));
        telephony.dispatch(cmd).await.ok();
      }
      ClientToBridgePhoneMsg::End(action) => {
        let cmd = TelephonyManager::build_end(0, Some(action.call_id));
        telephony.dispatch(cmd).await.ok();
      }
      ClientToBridgePhoneMsg::EndTyped(action) => {
        let action_byte = match action.action {
          EndCallAction::End => 0,
          EndCallAction::EndAll => 1,
        };
        let cmd = TelephonyManager::build_end(action_byte, Some(action.call_id));
        telephony.dispatch(cmd).await.ok();
      }
      ClientToBridgePhoneMsg::Hold(action) => {
        let cmd = TelephonyManager::build_hold(true, Some(action.call_id));
        telephony.dispatch(cmd).await.ok();
      }
      ClientToBridgePhoneMsg::Unhold(action) => {
        let cmd = TelephonyManager::build_hold(false, Some(action.call_id));
        telephony.dispatch(cmd).await.ok();
      }
      ClientToBridgePhoneMsg::Initiate(action) => {
        let kind = match action.kind {
          InitiateCallType::Destination => 0,
          InitiateCallType::Voicemail => 1,
          InitiateCallType::Redial => 2,
        };
        let service = action.service.map(|s| match s {
          PhoneCallService::Telephony => 1,
          PhoneCallService::FaceTimeAudio => 2,
          PhoneCallService::FaceTimeVideo => 3,
          PhoneCallService::Unknown => 0,
        });
        let cmd = TelephonyManager::build_initiate(kind, action.destination_id, service, action.address_book_id);
        telephony.dispatch(cmd).await.ok();
      }
      ClientToBridgePhoneMsg::Swap => {
        telephony
          .dispatch(bridgething_iap2::session::TelephonyCommand::Swap)
          .await
          .ok();
      }
      ClientToBridgePhoneMsg::Merge => {
        telephony
          .dispatch(bridgething_iap2::session::TelephonyCommand::Merge)
          .await
          .ok();
      }
      ClientToBridgePhoneMsg::Mute(action) => {
        let cmd = TelephonyManager::build_mute(action.mute);
        telephony.dispatch(cmd).await.ok();
      }
      ClientToBridgePhoneMsg::Dtmf(action) => {
        let cmd = TelephonyManager::build_dtmf(action.tone, action.call_id);
        telephony.dispatch(cmd).await.ok();
      }
      ClientToBridgePhoneMsg::StateGet => {
        let state = telephony.snapshot().await;
        self
          .handle
          .respond(BridgeToClientPhoneMsg::StateReply(PhoneStateReply { state }))
          .await?;
      }
    }
    Ok(())
  }
}
