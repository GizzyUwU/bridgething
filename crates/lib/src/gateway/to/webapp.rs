use bridgething_macros::BridgeEnum;
use derive_more::derive::Debug;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use typeshare::typeshare;
use uuid::Uuid;

use crate::{
  ArtProfile, ConfigEntry, DocEntry, WebappError, WebappInfo,
  gateway::{TransferBody, WebappResourceKind},
};

/// Reply to `WebappResource`. `sha256` is the current content hash;
/// `body` is absent when the requester's `have` already matched (its
/// cache is current). Large bodies stream as fragments per `TransferBody`.
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct WebappResourceReply {
  #[ts(type = "string")]
  #[typeshare(serialized_as = "Vec<u8>")]
  pub id: Uuid,
  pub kind: WebappResourceKind,
  pub sha256: String,
  pub mime: Option<String>,
  pub body: Option<TransferBody>,
}

#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct WebappConfigGetReply {
  pub key: String,
  pub value: Option<String>,
}

#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct WebappConfigListReply {
  pub entries: Vec<ConfigEntry>,
}

/// Ack for WebappConfigSet / WebappConfigDelete. The `value` field
/// echoes what's now stored after the write (None for delete).
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct WebappConfigAck {
  pub key: String,
  pub value: Option<String>,
}

#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct WebappDocGetReply {
  pub key: String,
  /// `None` when the key has never been written.
  pub value: Option<String>,
}

#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct WebappDocListReply {
  pub entries: Vec<DocEntry>,
}

/// Ack for WebappDocSet / WebappDocDelete; echoes what's now stored.
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct WebappDocAck {
  pub key: String,
  pub value: Option<String>,
}

/// Broadcast when the WEBAPP writes its doc namespace, so an open
/// companion settings page sees device-side changes live. Gateway-origin
/// writes are not echoed back (the writer already holds the ack).
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct WebappDocChanged {
  #[ts(type = "string")]
  #[typeshare(serialized_as = "Vec<u8>")]
  pub id: Uuid,
  pub key: String,
  pub value: Option<String>,
}

#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct WebappList {
  pub webapps: Vec<WebappInfo>,
}

#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct WebappActive {
  #[ts(type = "string | null")]
  #[typeshare(serialized_as = "Option<Vec<u8>>")]
  pub id: Option<Uuid>,
  pub name: Option<String>,
}

/// Event payload for an active-webapp change (any initiator). Distinct from
/// `WebappActive` (a request response) so it carries the new app's declared
/// art profile; the companion reads `art` directly to size its pushes.
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct WebappActiveChanged {
  #[ts(type = "string | null")]
  #[typeshare(serialized_as = "Option<Vec<u8>>")]
  pub id: Option<Uuid>,
  pub name: Option<String>,
  pub art: Option<ArtProfile>,
}

#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS, BridgeEnum)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
#[bridge_enum(into = crate::gateway::BridgeToGatewayMsgData)]
pub enum BridgeToGatewayWebappMsg {
  /// response to List
  #[bridge_response]
  Webapps(WebappList),
  /// response to GetActive, and event broadcast on switch
  #[bridge_response]
  Active(WebappActive),
  /// response to SwitchTo indicating the new active app
  #[bridge_response]
  Switched(WebappActive),
  /// response to Uninstall carrying the active app after the uninstall settled
  #[bridge_response]
  Uninstalled(WebappActive),
  /// domain-level error response for any webapp op (e.g. WebappNotFound,
  /// CannotUninstallBuiltin, IdReserved)
  #[bridge_response]
  WebappError(WebappError),
  #[bridge_response]
  Resource(WebappResourceReply),
  #[bridge_response]
  ConfigGet(WebappConfigGetReply),
  #[bridge_response]
  ConfigList(WebappConfigListReply),
  #[bridge_response]
  ConfigAck(WebappConfigAck),
  #[bridge_response]
  DocGet(WebappDocGetReply),
  #[bridge_response]
  DocList(WebappDocListReply),
  #[bridge_response]
  DocAck(WebappDocAck),
  /// event: the active webapp wrote/deleted a doc key
  #[bridge_event]
  DocChanged(WebappDocChanged),
  /// event: a webapp install (`OtaKind::InstalledWebapp`) completed
  /// successfully; carries the installed webapp's metadata. The terminal
  /// signal for an install; failures surface as `OtaError` on the system
  /// surface.
  #[bridge_event]
  WebappInstalled(WebappInfo),
  /// event: the active webapp changed (any initiator - hub tap, gateway
  /// switchTo, uninstall fallback). carries the new app's id/name + declared
  /// art profile so the companion sizes art pushes to what it renders.
  #[bridge_event]
  ActiveChanged(WebappActiveChanged),
}
