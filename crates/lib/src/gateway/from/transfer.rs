use bridgething_macros::BridgeEnum;
use derive_more::derive::Debug;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use typeshare::typeshare;
use uuid::Uuid;

use crate::gateway::TransferAck;

/// Handle to a fragment stream, embedded in the typed message that
/// opens a transfer (a pull reply or a push begin). The bytes travel
/// out-of-band as `TransferFragment` events keyed by `id`; the
/// transfer completes when `total_size` bytes have arrived. For pull
/// replies `id` is the originating request id.
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct TransferRef {
  #[ts(type = "string")]
  #[typeshare(serialized_as = "Vec<u8>")]
  pub id: Uuid,
  pub total_size: u32,
  pub sha256: Option<String>,
}

/// Standard embedding for a byte payload that may or may not warrant a
/// fragment stream. Senders pick by size: a small payload rides inline
/// in the carrying message (one frame, no machinery); a large one
/// declares a stream and fragments follow. Receivers resolve both arms
/// through one path.
#[typeshare]
#[serde_with::serde_as]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(tag = "type", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub enum TransferBody {
  Inline(
    #[debug(skip)]
    #[serde_as(as = "serde_with::Bytes")]
    #[ts(type = "Uint8Array")]
    Vec<u8>,
  ),
  Stream(TransferRef),
}

/// One slice of a transfer's bytes. Variable-size and offset-addressed:
/// fragments are sent in offset order on the transfer's priority lane,
/// sized by the sender to its preemption budget. Receivers route by
/// `transfer_id` to the sink bound when the transfer opened.
#[typeshare]
#[serde_with::serde_as]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct TransferFragment {
  #[ts(type = "string")]
  #[typeshare(serialized_as = "Vec<u8>")]
  pub transfer_id: Uuid,
  pub offset: u32,
  #[debug(skip)]
  #[serde_as(as = "serde_with::Bytes")]
  #[ts(type = "Uint8Array")]
  pub bytes: Vec<u8>,
}

/// Sender-side abort of an in-flight transfer (source lost the bytes,
/// upstream fetch failed). The receiver drops the bound sink; partial
/// disk state is kept for resumable transfers and discarded otherwise.
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct TransferAbandon {
  #[ts(type = "string")]
  #[typeshare(serialized_as = "Vec<u8>")]
  pub transfer_id: Uuid,
  pub reason: String,
}

#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, BridgeEnum)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
#[bridge_enum(into = crate::gateway::GatewayToBridgeMsgData)]
pub enum GatewayToBridgeTransferMsg {
  #[bridge_event]
  Fragment(TransferFragment),
  #[bridge_event]
  Abandon(TransferAbandon),
  #[bridge_event]
  Ack(TransferAck),
}
