use bridgething_macros::BridgeEnum;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use typeshare::typeshare;
use uuid::Uuid;

use crate::gateway::{TransferAbandon, TransferFragment};

/// Receiver-side progress for an in-flight fragment stream: cumulative contiguous bytes received.
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct TransferAck {
  #[ts(type = "string")]
  #[typeshare(serialized_as = "Vec<u8>")]
  pub transfer_id: Uuid,
  pub received: u32,
}

#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, BridgeEnum)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
#[bridge_enum(into = crate::gateway::BridgeToGatewayMsgData)]
pub enum BridgeToGatewayTransferMsg {
  #[bridge_event]
  Ack(TransferAck),
  #[bridge_event]
  Fragment(TransferFragment),
  #[bridge_event]
  Abandon(TransferAbandon),
}
