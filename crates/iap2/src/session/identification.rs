//! Identification flow: drives the `0x1D00..0x1D03` exchange.
//! Receives `StartIdentification`, replies with the
//! `IdentificationInformation` that wraps the caller's
//! [`IdentificationConfig`], then waits for accept-or-reject.

use tokio::sync::mpsc;

use crate::csm::{
  CsmFrame,
  identification::{
    IdentificationAccepted, IdentificationConfig, IdentificationInformation, IdentificationRejected,
    StartIdentification,
  },
};
use crate::error::Result;
use crate::link::Iap2Command;

use super::{SessionEvent, emit, send_csm};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum IdentState {
  AwaitingStart,
  AwaitingResult,
  Accepted,
  Rejected,
}

pub(super) struct IdentificationFlow {
  state: IdentState,
}

impl IdentificationFlow {
  pub(super) fn new() -> Self {
    Self {
      state: IdentState::AwaitingStart,
    }
  }

  pub(super) fn handles(msg_id: u16) -> bool {
    (0x1D00..=0x1D03).contains(&msg_id)
  }

  /// Process one identification-range CSM. Same return-shape contract
  /// as [`AuthFlow::handle`]: `Some` means "terminal, emit + tear down."
  ///
  /// [`AuthFlow::handle`]: super::auth::AuthFlow::handle
  pub(super) async fn handle(
    &mut self,
    frame: CsmFrame,
    identification: &IdentificationConfig,
    link_command_tx: &mpsc::Sender<Iap2Command>,
    session_events_tx: &mpsc::Sender<SessionEvent>,
  ) -> Result<Option<SessionEvent>> {
    match frame.msg_id {
      StartIdentification::CSM_MSG_ID => {
        let _: StartIdentification = frame.try_into()?;
        tracing::debug!("iap2 ident: replying IdentificationInformation");
        send_csm(
          IdentificationInformation {
            config: identification.clone(),
          },
          link_command_tx,
        )
        .await?;
        self.state = IdentState::AwaitingResult;
        Ok(None)
      }
      IdentificationAccepted::CSM_MSG_ID => {
        let _: IdentificationAccepted = frame.try_into()?;
        tracing::info!("iap2 ident: accepted");
        self.state = IdentState::Accepted;
        emit(session_events_tx, SessionEvent::Identified).await;
        Ok(None)
      }
      IdentificationRejected::CSM_MSG_ID => {
        let rejected: IdentificationRejected = frame.try_into()?;
        tracing::warn!(?rejected.rejected_params, "iap2 ident: rejected");
        self.state = IdentState::Rejected;
        Ok(Some(SessionEvent::IdentificationRejected {
          rejected_params: rejected.rejected_params,
        }))
      }
      other => {
        tracing::trace!(
          msg_id = format!("{other:#06x}"),
          "iap2 ident: ignoring CSM outside ident range"
        );
        Ok(None)
      }
    }
  }
}
