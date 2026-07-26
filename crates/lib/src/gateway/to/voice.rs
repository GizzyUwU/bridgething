use bridgething_macros::BridgeEnum;
use derive_more::derive::Debug;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use typeshare::typeshare;
use uuid::Uuid;

use crate::{VoiceDispatchErrorCode, VoiceDispatchTarget};

#[typeshare]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct VoiceFormat {
  pub sample_rate_hz: u32,
  pub channels: u16,
  pub bits_per_sample: u16,
}

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

#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct VoiceDispatchFailed {
  pub code: VoiceDispatchErrorCode,
  pub intent: String,
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
