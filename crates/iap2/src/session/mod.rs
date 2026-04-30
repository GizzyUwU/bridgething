//! iAP2 control-session orchestration.
//!
//! Sits above the link layer: subscribes to `Iap2Event` from a running
//! [`Link`], dispatches inbound CSMs to per-feature flows, and emits
//! [`SessionEvent`] upstream. Currently the auth + identification
//! flows ship; later wedges (NowPlaying, HID, EA dispatch) layer on
//! by adding a sibling `Flow` struct and threading it through
//! [`Iap2Session::handle_csm`]'s dispatcher.
//!
//! Auth requires the MFi coprocessor; the chip is reached through the
//! [`MfiAccess`] trait so production wires
//! [`WorkerMfiAccess`] (a dedicated thread around `MfiAuth<LinuxI2c>`)
//! and tests pass a fake. The session only invokes `cert()` once per
//! RFCOMM connection and `sign()` once per challenge - per cleanroom
//! doc `protocol/50_authentication.md` we must not retry auth on the
//! same connection.
//!
//! Failure paths uniformly emit a terminal `SessionEvent::LinkDown`
//! (or `AuthFailed` / `IdentificationRejected` followed by a
//! `LinkDown`) before the task exits, so consumers only need to watch
//! the event channel; the `JoinHandle::Result` is informational.
//!
//! [`Link`]: crate::Link

mod auth;
mod identification;
mod mfi_worker;

use async_trait::async_trait;
use bridgething_mfi::{CHALLENGE_LEN, Error as MfiError, RESPONSE_LEN};
use bytes::{Bytes, BytesMut};
use tokio::sync::mpsc;
use tokio_util::codec::{Decoder, Encoder};

use crate::csm::{CsmCodec, CsmFrame, identification::IdentificationConfig};
use crate::error::{Error, Result};
use crate::frame::Lsp;
use crate::link::{Iap2Command, Iap2Event};

use auth::AuthFlow;
use identification::IdentificationFlow;

pub use mfi_worker::WorkerMfiAccess;

/// Result alias for `MfiAccess` ops; uses the mfi crate's error type
/// directly since the iap2 layer only translates the result, doesn't
/// need to wrap.
pub type MfiResult<T> = std::result::Result<T, MfiError>;

/// Async-trait surface over the MFi coprocessor. Two methods, one
/// production impl ([`WorkerMfiAccess`]), and tests fake their own.
#[async_trait]
pub trait MfiAccess: Send + 'static {
  async fn cert(&mut self) -> MfiResult<Bytes>;
  async fn sign(&mut self, challenge: [u8; CHALLENGE_LEN]) -> MfiResult<[u8; RESPONSE_LEN]>;
}

/// iAP2 control-session id. Auth + identification ride session 0; EA
/// sessions get distinct ids assigned at session-start time.
pub(crate) const CONTROL_SESSION_ID: u8 = 0;

/// Events the session emits upstream. `LinkEstablished` carries the
/// peer's negotiated LSP for any consumer that wants to log or
/// surface negotiated parameters. `LinkDown` is always the final
/// event before the task exits (success, peer disconnect, auth/ident
/// failure, or hard error - all paths emit it).
#[derive(Debug, Clone)]
pub enum SessionEvent {
  LinkEstablished(Lsp),
  Authenticated,
  Identified,
  AuthFailed,
  IdentificationRejected { rejected_params: Vec<u16> },
  LinkDown(String),
}

/// Top-level iAP2 session task. Constructed with the running link's
/// event/command channels plus an [`MfiAccess`] impl and an
/// [`IdentificationConfig`]; drive with `run().await`. Always emits a
/// terminal `SessionEvent::LinkDown` before returning, regardless of
/// success or failure path.
pub struct Iap2Session<M: MfiAccess> {
  identification: IdentificationConfig,
  mfi: M,
  link_command_tx: mpsc::Sender<Iap2Command>,
  link_events_rx: mpsc::Receiver<Iap2Event>,
  session_events_tx: mpsc::Sender<SessionEvent>,
  auth: AuthFlow,
  ident: IdentificationFlow,
}

impl<M: MfiAccess> Iap2Session<M> {
  pub fn new(
    identification: IdentificationConfig,
    mfi: M,
    link_command_tx: mpsc::Sender<Iap2Command>,
    link_events_rx: mpsc::Receiver<Iap2Event>,
    session_events_tx: mpsc::Sender<SessionEvent>,
  ) -> Self {
    Self {
      identification,
      mfi,
      link_command_tx,
      link_events_rx,
      session_events_tx,
      auth: AuthFlow::new(),
      ident: IdentificationFlow::new(),
    }
  }

  pub async fn run(mut self) -> Result<()> {
    match self.run_inner().await {
      Ok(()) => Ok(()),
      Err(err) => {
        tracing::error!(?err, "iap2 session ended in error");
        self.disconnect_link().await;
        emit(
          &self.session_events_tx,
          SessionEvent::LinkDown(format!("session error: {err}")),
        )
        .await;
        Err(err)
      }
    }
  }

  async fn run_inner(&mut self) -> Result<()> {
    let mut control_buf = BytesMut::new();

    loop {
      while let Some(frame) = CsmCodec.decode(&mut control_buf)? {
        if let Some(reason) = self.handle_csm(frame).await? {
          self.disconnect_link().await;
          emit(&self.session_events_tx, reason).await;
        }
      }

      match self.link_events_rx.recv().await {
        Some(Iap2Event::Established(lsp)) => {
          tracing::debug!("iap2 session: link established");
          emit(&self.session_events_tx, SessionEvent::LinkEstablished(lsp)).await;
        }
        Some(Iap2Event::DataReceived { session_id, payload }) => {
          if session_id == CONTROL_SESSION_ID {
            control_buf.extend_from_slice(&payload);
          } else {
            tracing::trace!(session_id, "iap2 session: ignoring data on non-control session");
          }
        }
        Some(Iap2Event::LinkDown(reason)) => {
          tracing::info!(reason = %reason, "iap2 session: link down");
          emit(&self.session_events_tx, SessionEvent::LinkDown(reason)).await;
          return Ok(());
        }
        None => {
          tracing::debug!("iap2 session: link events channel closed");
          emit(
            &self.session_events_tx,
            SessionEvent::LinkDown("link task exited".into()),
          )
          .await;
          return Ok(());
        }
      }
    }
  }

  async fn handle_csm(&mut self, frame: CsmFrame) -> Result<Option<SessionEvent>> {
    let msg_id = frame.msg_id;
    if AuthFlow::handles(msg_id) {
      return self
        .auth
        .handle(frame, &mut self.mfi, &self.link_command_tx, &self.session_events_tx)
        .await;
    }
    if IdentificationFlow::handles(msg_id) {
      if !self.auth.is_authenticated() && msg_id == crate::csm::identification::StartIdentification::CSM_MSG_ID {
        tracing::warn!("iap2 session: StartIdentification before AuthenticationSucceeded");
      }
      return self
        .ident
        .handle(
          frame,
          &self.identification,
          &self.link_command_tx,
          &self.session_events_tx,
        )
        .await;
    }
    tracing::trace!(msg_id = format!("{msg_id:#06x}"), "iap2 session: unhandled CSM");
    Ok(None)
  }

  async fn disconnect_link(&self) {
    if self.link_command_tx.send(Iap2Command::Disconnect).await.is_err() {
      tracing::debug!("iap2 session: link command channel closed before Disconnect could be sent");
    }
  }
}

/// Encode `csm` and dispatch it as a link `Send` on the control
/// session. Shared by every flow; `pub(super)` so flow modules don't
/// need to re-implement encode bookkeeping.
pub(super) async fn send_csm<F>(csm: F, link_command_tx: &mpsc::Sender<Iap2Command>) -> Result<()>
where
  F: Into<CsmFrame>,
{
  let frame: CsmFrame = csm.into();
  let mut buf = BytesMut::new();
  CsmCodec.encode(frame, &mut buf)?;
  link_command_tx
    .send(Iap2Command::Send {
      session_id: CONTROL_SESSION_ID,
      payload: buf.freeze(),
    })
    .await
    .map_err(|_| Error::LinkClosed)?;
  Ok(())
}

/// Best-effort emit; logs at debug if the receiver is gone (which is
/// recoverable - the session keeps running, the consumer just won't
/// see the event).
pub(super) async fn emit(tx: &mpsc::Sender<SessionEvent>, event: SessionEvent) {
  if tx.send(event).await.is_err() {
    tracing::debug!("iap2 session: events receiver dropped");
  }
}
