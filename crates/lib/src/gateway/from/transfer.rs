use bridgething_macros::BridgeEnum;
use bytes::Bytes;
use derive_more::derive::Debug;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::gateway::TransferAck;

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct TransferRef {
  #[ts(type = "string")]
  pub id: Uuid,
  pub total_size: u32,
  pub sha256: Option<String>,
}

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

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct TransferFragment {
  #[ts(type = "string")]
  pub transfer_id: Uuid,
  pub offset: u32,
  #[debug(skip)]
  #[ts(type = "Uint8Array")]
  pub bytes: Bytes,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct TransferAbandon {
  #[ts(type = "string")]
  pub transfer_id: Uuid,
  pub reason: String,
}

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
