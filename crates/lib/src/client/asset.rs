use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::{
  client::ClientCommandType,
  impl_client_request,
  server::{AssetGot, ServerAssetEvent, ServerEventData},
};

/// Webapp-side asset operations. `Get` is request-style and resolves with
/// the bytes when the asset is in cache (or arrives shortly after via a
/// daemon-issued `AssetRequest` to the companion). The `request_id`
/// matches the bytes back to the originating Get.
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(
  tag = "action",
  content = "args",
  rename_all = "camelCase",
  rename_all_fields = "camelCase"
)]
#[ts(export, export_to = "client.ts")]
pub enum ClientAssetCommand {
  Get {
    id: String,
    #[ts(type = "string")]
    request_id: Uuid,
  },
}

/// Newtype Get used by the typed-request macro - same wire shape as the
/// `Get` variant above, separated only so it can be a request type by
/// itself.
#[derive(Debug, Clone)]
pub struct AssetGet {
  pub id: String,
  pub request_id: Uuid,
}

impl_client_request! {
  request: AssetGet,
  response: AssetGot,
  encode_request:
    r => ClientCommandType::Asset(ClientAssetCommand::Get { id: r.id, request_id: r.request_id }),
  extract_response:
    ServerEventData::Asset(ServerAssetEvent::Got(v)) => v,
  encode_response:
    v => ServerEventData::Asset(ServerAssetEvent::Got(v)),
}
