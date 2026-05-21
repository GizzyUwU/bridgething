use bridgething_macros::BridgeEnum;
use derive_more::derive::Debug;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use typeshare::typeshare;
use uuid::Uuid;

use crate::{VoiceDispatchErrorCode, VoiceDispatchTarget};

/// PCM frame format the daemon ships in `Frame` payloads. Voice capture
/// runs at a fixed format per session; format is announced once on
/// `StreamOpen` and held constant through `StreamClose`.
#[typeshare]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct VoiceFormat {
  pub sample_rate_hz: u32,
  pub channels: u16,
  pub bits_per_sample: u16,
}

/// Daemon opens a capture session. The companion is expected to begin
/// consuming `Frame`s with the same `stream_id` until a `StreamClose`
/// for that id arrives.
#[typeshare]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct VoiceStreamOpen {
  #[ts(type = "string")]
  #[typeshare(serialized_as = "Vec<u8>")]
  pub stream_id: Uuid,
  pub format: VoiceFormat,
}

/// One PCM frame in an active capture session. Sent on the Bulk lane so
/// it interleaves between Normal-priority traffic. `seq` increments
/// from 0; gaps mean the daemon dropped frames under backpressure and
/// the companion should treat them as silence rather than retransmit.
#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct VoiceFrame {
  #[ts(type = "string")]
  #[typeshare(serialized_as = "Vec<u8>")]
  pub stream_id: Uuid,
  pub seq: u32,
  #[debug(skip)]
  #[ts(type = "Uint8Array")]
  #[typeshare(serialized_as = "Vec<u8>")]
  pub pcm: bytes::Bytes,
}

#[typeshare]
#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub enum VoiceCloseReason {
  #[default]
  EndOfSpeech,
  Cancelled,
  Muted,
  Error,
}

#[typeshare]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct VoiceStreamClose {
  #[ts(type = "string")]
  #[typeshare(serialized_as = "Vec<u8>")]
  pub stream_id: Uuid,
  pub reason: VoiceCloseReason,
}

/// Notification that an inbound `VoiceDispatch` was routed. `target`
/// describes where the daemon sent it; the daemon's own action surfaces
/// (Player events, WebappActive changes) are the source of truth for the
/// effect - this event is purely so the companion can render confirmation
/// UI without polling state.
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct VoiceDispatched {
  pub target: VoiceDispatchTarget,
  pub intent: String,
  pub webapp_id: Option<String>,
}

/// Terminal failure for an inbound `VoiceDispatch`. Daemon could not
/// route the resolved intent; companion presents the appropriate UX.
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct VoiceDispatchFailed {
  pub code: VoiceDispatchErrorCode,
  pub intent: String,
  pub webapp_id: Option<String>,
  pub msg: String,
}

#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS, BridgeEnum)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
#[bridge_enum(into = crate::gateway::BridgeToGatewayMsgData)]
pub enum BridgeToGatewayVoiceMsg {
  #[bridge_event]
  StreamOpen(VoiceStreamOpen),
  #[bridge_event]
  Frame(VoiceFrame),
  #[bridge_event]
  StreamClose(VoiceStreamClose),
  #[bridge_event]
  Dispatched(VoiceDispatched),
  #[bridge_event]
  DispatchFailed(VoiceDispatchFailed),
}
