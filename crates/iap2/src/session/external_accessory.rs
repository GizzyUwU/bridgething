//! External Accessory flow: bridges iAP2 link session_id 3 (the
//! ExternalAccessory link session declared in
//! `Lsp::accessory_default`) plus the four `0xEA0x` control-session
//! CSMs into a clean per-EA-stream byte-channel surface for upstream
//! consumers.
//!
//! Inbound `StartExternalAccessoryProtocolSession` opens a per-stream
//! state, replies with a `StatusExternalAccessoryProtocolSession::Ok`,
//! and emits `SessionEvent::EaStreamOpened` carrying the byte
//! channels the consumer will read/write. Inbound link DATA on
//! session_id 3 is split by the leading u16-BE EA-stream-id and
//! forwarded into the matching per-stream inbound channel. Outbound
//! traffic from the consumer rides two priority lanes (Normal and
//! Bulk); a chunker task drains them with Normal-first preference,
//! splits each frame at the iAP2 link's `max_len - 2` budget so the
//! u16 stream-id prefix fits, and dispatches one
//! `Iap2Command::Send { session_id: 3, ... }` per chunk. Between
//! chunks the chunker re-checks the Normal lane so a control-plane
//! frame can preempt a long bulk transfer at chunk boundaries (the
//! atomic unit of priority preemption is one iAP2 link DATA packet).
//!
//! Stream state lives in [`EaFlow`]; the chunker is a sibling task
//! spawned alongside the session and shares the same Iap2Command
//! sender. Stream close (peer Stop, link tear-down, or the consumer
//! dropping the channel ends) tears down the per-stream state and
//! emits `SessionEvent::EaStreamClosed`.
//!
//! `ensure_app_launch_requested` is the post-Identified hook the
//! session calls once: it dispatches `RequestAppLaunch` with the
//! configured bundle id (typically `com.bridgething.gateway`). iOS
//! either foregrounds the matching app, opens a Settings deeplink
//! (per the protocol's `match_action`), or silently no-ops if the app
//! isn't installed. Idempotent; subsequent calls are no-ops.

use std::collections::HashMap;

use bytes::{Bytes, BytesMut};
use tokio::{sync::mpsc, task::JoinHandle};

use super::{SessionEvent, emit, send_csm};
use crate::{
  csm::{
    CsmFrame, external_accessory::{
      EaSessionStatus, RequestAppLaunch, StartExternalAccessoryProtocolSession, StatusExternalAccessoryProtocolSession,
      StopExternalAccessoryProtocolSession,
    },
  },
  error::Result,
  link::Iap2Command,
};

/// Link session id used by `Lsp::accessory_default` for EA traffic.
/// Must match the `SessionTriple { session_type: 2, ... }` we declare
/// in our SYN.
pub(crate) const EA_LINK_SESSION_ID: u8 = 3;

/// Lane priority hint a consumer attaches when sending bytes on an EA
/// stream. Mirrors `libbridgething::Priority` semantically but is
/// kept crate-local so the iap2 crate stays independent of lib.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum EaPriority {
  #[default]
  Normal,
  Bulk,
}

const LANE_CAPACITY: usize = 16;
const STREAM_INBOUND_CAPACITY: usize = 32;

type FramedBytes = (u16, Bytes);

/// Outbound side of an EA stream. The consumer pre-binds its
/// stream id when an `EaStreamOpened` event fires; calling
/// [`EaStreamSender::send`] tags each frame with that stream id and
/// posts it to the matching priority lane on the chunker's fan-in.
#[derive(Debug, Clone)]
pub struct EaStreamSender {
  stream_id: u16,
  normal_tx: mpsc::Sender<FramedBytes>,
  bulk_tx: mpsc::Sender<FramedBytes>,
}

impl EaStreamSender {
  pub fn stream_id(&self) -> u16 {
    self.stream_id
  }

  pub async fn send(&self, priority: EaPriority, frame: Bytes) -> std::result::Result<(), EaSendError> {
    let lane = match priority {
      EaPriority::Normal => &self.normal_tx,
      EaPriority::Bulk => &self.bulk_tx,
    };
    lane
      .send((self.stream_id, frame))
      .await
      .map_err(|_| EaSendError::ChannelClosed)
  }
}

#[derive(Debug, thiserror::Error)]
pub enum EaSendError {
  #[error("EA chunker is no longer running for this link")]
  ChannelClosed,
}

#[derive(Debug)]
struct EaStreamState {
  protocol_id: u8,
  inbound_tx: mpsc::Sender<Bytes>,
  reassembly: BytesMut,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AppLaunchState {
  Idle,
  Requested,
}

pub(super) struct EaFlow {
  streams: HashMap<u16, EaStreamState>,
  normal_tx: mpsc::Sender<FramedBytes>,
  bulk_tx: mpsc::Sender<FramedBytes>,
  app_launch: AppLaunchState,
  _chunker_handle: JoinHandle<()>,
}

impl std::fmt::Debug for EaFlow {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("EaFlow")
      .field("streams", &self.streams.keys().collect::<Vec<_>>())
      .field("app_launch", &self.app_launch)
      .finish()
  }
}

impl EaFlow {
  pub(super) fn new(link_command_tx: mpsc::Sender<Iap2Command>, peer_max_len: u16) -> Self {
    let (normal_tx, normal_rx) = mpsc::channel(LANE_CAPACITY);
    let (bulk_tx, bulk_rx) = mpsc::channel(LANE_CAPACITY);
    let max_chunk_payload = max_chunk_payload(peer_max_len);
    let _chunker_handle = tokio::spawn(chunker_task(normal_rx, bulk_rx, link_command_tx, max_chunk_payload));
    Self {
      streams: HashMap::new(),
      normal_tx,
      bulk_tx,
      app_launch: AppLaunchState::Idle,
      _chunker_handle,
    }
  }

  pub(super) fn handles(msg_id: u16) -> bool {
    msg_id == StartExternalAccessoryProtocolSession::CSM_MSG_ID
      || msg_id == StopExternalAccessoryProtocolSession::CSM_MSG_ID
  }

  /// Idempotent post-Identified kick. Sends `RequestAppLaunch` once
  /// per session; subsequent calls are no-ops. The bundle id should
  /// match an app declaring our EA protocol string in its
  /// `UISupportedExternalAccessoryProtocols` Info.plist key. iOS
  /// silently ignores the request if the app isn't installed or
  /// doesn't list the protocol.
  pub(super) async fn ensure_app_launch_requested(
    &mut self,
    bundle_id: &str,
    link_command_tx: &mpsc::Sender<Iap2Command>,
  ) -> Result<()> {
    if matches!(self.app_launch, AppLaunchState::Idle) {
      tracing::debug!(bundle_id, "iap2 ea: sending RequestAppLaunch");
      send_csm(
        RequestAppLaunch {
          bundle_id: bundle_id.to_string(),
        },
        link_command_tx,
      )
      .await?;
      self.app_launch = AppLaunchState::Requested;
    }
    Ok(())
  }

  /// Dispatch one EA-range control CSM. Returns `Ok(None)` for the
  /// happy path; this layer never produces a terminal `SessionEvent`
  /// itself - if the link falls over, the link layer surfaces
  /// `LinkDown` directly.
  pub(super) async fn handle(
    &mut self,
    frame: CsmFrame,
    link_command_tx: &mpsc::Sender<Iap2Command>,
    session_events_tx: &mpsc::Sender<SessionEvent>,
  ) -> Result<Option<SessionEvent>> {
    match frame.msg_id {
      StartExternalAccessoryProtocolSession::CSM_MSG_ID => {
        let start: StartExternalAccessoryProtocolSession = frame.try_into()?;
        self.handle_start(start, link_command_tx, session_events_tx).await?;
      }
      StopExternalAccessoryProtocolSession::CSM_MSG_ID => {
        let stop: StopExternalAccessoryProtocolSession = frame.try_into()?;
        self.handle_stop(stop, session_events_tx).await;
      }
      _ => {}
    }
    Ok(None)
  }

  async fn handle_start(
    &mut self,
    start: StartExternalAccessoryProtocolSession,
    link_command_tx: &mpsc::Sender<Iap2Command>,
    session_events_tx: &mpsc::Sender<SessionEvent>,
  ) -> Result<()> {
    if self.streams.contains_key(&start.session_id) {
      tracing::warn!(
        stream_id = start.session_id,
        "iap2 ea: StartExternalAccessoryProtocolSession for a stream id already open; refusing"
      );
      send_csm(
        StatusExternalAccessoryProtocolSession {
          session_id: start.session_id,
          status: EaSessionStatus::Close,
        },
        link_command_tx,
      )
      .await?;
      return Ok(());
    }

    let (inbound_tx, inbound_rx) = mpsc::channel(STREAM_INBOUND_CAPACITY);
    self.streams.insert(
      start.session_id,
      EaStreamState {
        protocol_id: start.protocol_id,
        inbound_tx,
        reassembly: BytesMut::new(),
      },
    );

    send_csm(
      StatusExternalAccessoryProtocolSession {
        session_id: start.session_id,
        status: EaSessionStatus::Ok,
      },
      link_command_tx,
    )
    .await?;
    tracing::info!(
      stream_id = start.session_id,
      protocol_id = start.protocol_id,
      "iap2 ea: stream opened"
    );

    let outbound = EaStreamSender {
      stream_id: start.session_id,
      normal_tx: self.normal_tx.clone(),
      bulk_tx: self.bulk_tx.clone(),
    };
    emit(
      session_events_tx,
      SessionEvent::EaStreamOpened {
        stream_id: start.session_id,
        protocol_id: start.protocol_id,
        inbound_rx,
        outbound,
      },
    )
    .await;
    Ok(())
  }

  async fn handle_stop(
    &mut self,
    stop: StopExternalAccessoryProtocolSession,
    session_events_tx: &mpsc::Sender<SessionEvent>,
  ) {
    if self.streams.remove(&stop.session_id).is_some() {
      tracing::info!(stream_id = stop.session_id, "iap2 ea: stream closed by peer");
      emit(
        session_events_tx,
        SessionEvent::EaStreamClosed {
          stream_id: stop.session_id,
        },
      )
      .await;
    }
  }

  /// Strip the leading u16-BE EA-stream-id from a session_id=3 link
  /// payload and route the rest to the matching per-stream inbound
  /// channel. Drops chunks for stream ids we don't know about.
  pub(super) async fn dispatch_link_data(&mut self, payload: Bytes) {
    if payload.len() < 2 {
      tracing::warn!(
        len = payload.len(),
        "iap2 ea: link payload too short for stream-id prefix"
      );
      return;
    }
    let stream_id = u16::from_be_bytes([payload[0], payload[1]]);
    let chunk = payload.slice(2..);
    let Some(state) = self.streams.get_mut(&stream_id) else {
      tracing::trace!(stream_id, "iap2 ea: link payload for unknown stream id");
      return;
    };
    if state.inbound_tx.send(chunk).await.is_err() {
      tracing::debug!(stream_id, "iap2 ea: inbound consumer dropped; closing stream");
      self.streams.remove(&stream_id);
    }
  }
}

const fn max_chunk_payload(peer_max_len: u16) -> usize {
  // peer_max_len is the link's full DATA-packet budget. Each EA
  // chunk uses 2 bytes for the stream-id prefix; the remainder is
  // the payload we can hand to the iAP2 link layer.
  let total = peer_max_len as usize;
  if total <= 2 { 1 } else { total - 2 }
}

async fn chunker_task(
  mut normal_rx: mpsc::Receiver<FramedBytes>,
  mut bulk_rx: mpsc::Receiver<FramedBytes>,
  link_tx: mpsc::Sender<Iap2Command>,
  max_chunk_payload: usize,
) {
  let mut pending_normal: Option<FramedBytes> = None;
  let mut pending_bulk: Option<FramedBytes> = None;

  loop {
    if let Some((stream_id, mut bytes)) = pending_normal.take() {
      if !send_one_chunk(&link_tx, stream_id, &mut bytes, max_chunk_payload).await {
        return;
      }
      if !bytes.is_empty() {
        pending_normal = Some((stream_id, bytes));
      }
      continue;
    }

    if let Ok(frame) = normal_rx.try_recv() {
      pending_normal = Some(frame);
      continue;
    }

    if let Some((stream_id, mut bytes)) = pending_bulk.take() {
      if !send_one_chunk(&link_tx, stream_id, &mut bytes, max_chunk_payload).await {
        return;
      }
      if !bytes.is_empty() {
        pending_bulk = Some((stream_id, bytes));
      }
      continue;
    }

    if let Ok(frame) = bulk_rx.try_recv() {
      pending_bulk = Some(frame);
      continue;
    }

    let next = tokio::select! {
      biased;
      Some(f) = normal_rx.recv() => (Lane::Normal, f),
      Some(f) = bulk_rx.recv() => (Lane::Bulk, f),
      else => return,
    };
    match next.0 {
      Lane::Normal => pending_normal = Some(next.1),
      Lane::Bulk => pending_bulk = Some(next.1),
    }
  }
}

#[derive(Debug, Clone, Copy)]
enum Lane {
  Normal,
  Bulk,
}

async fn send_one_chunk(
  link_tx: &mpsc::Sender<Iap2Command>,
  stream_id: u16,
  bytes: &mut Bytes,
  max_chunk_payload: usize,
) -> bool {
  let take = bytes.len().min(max_chunk_payload);
  let chunk = bytes.split_to(take);
  let mut wire = BytesMut::with_capacity(2 + chunk.len());
  wire.extend_from_slice(&stream_id.to_be_bytes());
  wire.extend_from_slice(&chunk);
  link_tx
    .send(Iap2Command::Send {
      session_id: EA_LINK_SESSION_ID,
      payload: wire.freeze(),
    })
    .await
    .is_ok()
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::frame::Lsp;

  fn drain_chunks(rx: &mut mpsc::Receiver<Iap2Command>) -> Vec<Bytes> {
    let mut out = Vec::new();
    while let Ok(cmd) = rx.try_recv() {
      if let Iap2Command::Send { session_id, payload } = cmd {
        assert_eq!(session_id, EA_LINK_SESSION_ID);
        out.push(payload);
      }
    }
    out
  }

  fn assert_chunk(payload: &Bytes, expected_stream: u16, expected_data: &[u8]) {
    assert!(payload.len() >= 2);
    let stream = u16::from_be_bytes([payload[0], payload[1]]);
    assert_eq!(stream, expected_stream);
    assert_eq!(&payload[2..], expected_data);
  }

  #[tokio::test]
  async fn chunker_splits_large_frame() {
    let (link_tx, mut link_rx) = mpsc::channel(64);
    let (n_tx, n_rx) = mpsc::channel(8);
    let (_b_tx, b_rx) = mpsc::channel(8);
    tokio::spawn(chunker_task(n_rx, b_rx, link_tx, 4));

    let payload = Bytes::from_static(&[1, 2, 3, 4, 5, 6, 7, 8, 9]);
    n_tx.send((0x0100, payload)).await.unwrap();
    drop(n_tx);

    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    let chunks = drain_chunks(&mut link_rx);
    assert_eq!(chunks.len(), 3, "9 bytes / 4 chunk size = 3 chunks");
    assert_chunk(&chunks[0], 0x0100, &[1, 2, 3, 4]);
    assert_chunk(&chunks[1], 0x0100, &[5, 6, 7, 8]);
    assert_chunk(&chunks[2], 0x0100, &[9]);
  }

  #[tokio::test]
  async fn chunker_normal_preempts_bulk_at_chunk_boundary() {
    let (link_tx, mut link_rx) = mpsc::channel(64);
    let (n_tx, n_rx) = mpsc::channel(8);
    let (b_tx, b_rx) = mpsc::channel(8);
    tokio::spawn(chunker_task(n_rx, b_rx, link_tx, 4));

    // Queue a Bulk frame first so it will start flowing.
    b_tx
      .send((
        0x0200,
        Bytes::from_static(&[0xB0, 0xB1, 0xB2, 0xB3, 0xB4, 0xB5, 0xB6, 0xB7]),
      ))
      .await
      .unwrap();

    // Give the chunker a moment to fetch the Bulk frame.
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;

    // Now interject a Normal frame.
    n_tx.send((0x0100, Bytes::from_static(&[0xA0, 0xA1]))).await.unwrap();

    drop(n_tx);
    drop(b_tx);
    tokio::time::sleep(std::time::Duration::from_millis(40)).await;

    let chunks = drain_chunks(&mut link_rx);
    // First chunk is the head of Bulk (chunker had already pulled it);
    // then Normal preempts the next chunk; then remaining Bulk drains.
    let stream_seq: Vec<u16> = chunks.iter().map(|p| u16::from_be_bytes([p[0], p[1]])).collect();
    assert!(
      stream_seq.windows(2).any(|w| w[0] == 0x0200 && w[1] == 0x0100),
      "Normal stream chunk lands between Bulk chunks (got {:?})",
      stream_seq
    );
    let collected_bulk: Vec<u8> = chunks
      .iter()
      .filter(|p| u16::from_be_bytes([p[0], p[1]]) == 0x0200)
      .flat_map(|p| p[2..].to_vec())
      .collect();
    assert_eq!(collected_bulk, vec![0xB0, 0xB1, 0xB2, 0xB3, 0xB4, 0xB5, 0xB6, 0xB7]);
  }

  #[tokio::test]
  async fn flow_handles_start_stop_lifecycle() {
    let (link_tx, mut link_rx) = mpsc::channel(64);
    let (events_tx, mut events_rx) = mpsc::channel(64);
    let lsp = Lsp::accessory_default();
    let mut flow = EaFlow::new(link_tx.clone(), lsp.max_len);

    let start_frame: CsmFrame = StartExternalAccessoryProtocolSession {
      protocol_id: 1,
      session_id: 0x0100,
    }
    .into();
    flow.handle(start_frame, &link_tx, &events_tx).await.unwrap();

    let event = events_rx.recv().await.unwrap();
    let opened_outbound = match event {
      SessionEvent::EaStreamOpened {
        stream_id,
        protocol_id,
        outbound,
        ..
      } => {
        assert_eq!(stream_id, 0x0100);
        assert_eq!(protocol_id, 1);
        outbound
      }
      other => panic!("unexpected event: {other:?}"),
    };

    // Status reply should have been sent on the link command channel.
    let status_cmd = link_rx.recv().await.unwrap();
    let Iap2Command::Send {
      session_id: status_session,
      ..
    } = status_cmd
    else {
      panic!("expected Send for status reply");
    };
    assert_eq!(
      status_session, 1,
      "status reply rides the control session, not the EA session"
    );

    opened_outbound
      .send(EaPriority::Normal, Bytes::from_static(&[0xCA, 0xFE]))
      .await
      .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    let chunk_cmd = link_rx.recv().await.unwrap();
    let Iap2Command::Send {
      session_id: ea_session,
      payload,
    } = chunk_cmd
    else {
      panic!("expected Send for EA chunk");
    };
    assert_eq!(ea_session, EA_LINK_SESSION_ID);
    assert_chunk(&payload, 0x0100, &[0xCA, 0xFE]);

    let stop_frame: CsmFrame = StopExternalAccessoryProtocolSession { session_id: 0x0100 }.into();
    flow.handle(stop_frame, &link_tx, &events_tx).await.unwrap();
    let event = events_rx.recv().await.unwrap();
    assert!(matches!(event, SessionEvent::EaStreamClosed { stream_id: 0x0100 }));
  }

  #[tokio::test]
  async fn dispatch_routes_inbound_payload_into_stream_channel() {
    let (link_tx, _link_rx) = mpsc::channel(64);
    let (events_tx, mut events_rx) = mpsc::channel(64);
    let mut flow = EaFlow::new(link_tx.clone(), Lsp::accessory_default().max_len);

    let start_frame: CsmFrame = StartExternalAccessoryProtocolSession {
      protocol_id: 1,
      session_id: 0x0100,
    }
    .into();
    flow.handle(start_frame, &link_tx, &events_tx).await.unwrap();

    let mut inbound_rx = match events_rx.recv().await.unwrap() {
      SessionEvent::EaStreamOpened { inbound_rx, .. } => inbound_rx,
      other => panic!("unexpected event: {other:?}"),
    };

    let mut wire = BytesMut::new();
    wire.extend_from_slice(&0x0100u16.to_be_bytes());
    wire.extend_from_slice(&[0xDE, 0xAD]);
    flow.dispatch_link_data(wire.freeze()).await;

    let chunk = inbound_rx.recv().await.unwrap();
    assert_eq!(&chunk[..], &[0xDE, 0xAD]);
  }

  #[tokio::test]
  async fn ensure_app_launch_is_idempotent() {
    let (link_tx, mut link_rx) = mpsc::channel(64);
    let mut flow = EaFlow::new(link_tx.clone(), Lsp::accessory_default().max_len);

    flow
      .ensure_app_launch_requested("com.bridgething.gateway", &link_tx)
      .await
      .unwrap();
    flow
      .ensure_app_launch_requested("com.bridgething.gateway", &link_tx)
      .await
      .unwrap();

    let mut launches = 0;
    while let Ok(cmd) = link_rx.try_recv() {
      if matches!(cmd, Iap2Command::Send { session_id: 1, .. }) {
        launches += 1;
      }
    }
    assert_eq!(launches, 1, "RequestAppLaunch sent exactly once");
  }
}
