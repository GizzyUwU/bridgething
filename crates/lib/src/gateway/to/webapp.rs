use bridgething_macros::BridgeEnum;
use derive_more::derive::Debug;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use typeshare::typeshare;
use uuid::Uuid;

use crate::{ConfigEntry, WebappError, WebappInfo};

#[typeshare]
#[serde_with::serde_as]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct WebappIconReply {
  #[debug(skip)]
  #[serde_as(as = "serde_with::Bytes")]
  #[ts(type = "Uint8Array")]
  pub bytes: Vec<u8>,
  pub mime: Option<String>,
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

/// Successful response to `WebappInstallBegin`. The companion's next
/// `WebappInstallChunk` should start at `resume_from_offset`; 0 for a
/// fresh push, or the byte count already on disk when resuming a
/// partial after disconnect.
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct WebappInstallBeginAck {
  pub resume_from_offset: u32,
}

/// Asynchronous failure of an in-flight install after the upload
/// completed (post-stream verify failed, extract failed, validation
/// failed, etc). Pairs with `WebappInstalled` as the terminal-event
/// duo for an install.
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub struct WebappInstallFailed {
  pub install_id: String,
  pub error: WebappError,
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
  /// response to InstallBegin indicating the resume offset for the next chunk
  #[bridge_response]
  InstallBeginAck(WebappInstallBeginAck),
  /// response to Uninstall carrying the active app after the uninstall settled
  #[bridge_response]
  Uninstalled(WebappActive),
  /// domain-level error response for any webapp op (e.g. WebappNotFound,
  /// CannotUninstallBuiltin, ArchiveTransferNotFound)
  #[bridge_response]
  WebappError(WebappError),
  #[bridge_response]
  Icon(WebappIconReply),
  #[bridge_response]
  ConfigGet(WebappConfigGetReply),
  #[bridge_response]
  ConfigList(WebappConfigListReply),
  #[bridge_response]
  ConfigAck(WebappConfigAck),
  /// event: a chunked install completed successfully; carries the
  /// installed webapp's metadata.
  #[bridge_event]
  WebappInstalled(WebappInfo),
  /// event: a chunked install failed post-upload (verify / extract /
  /// validate); carries the install_id and a typed `WebappError`.
  #[bridge_event]
  WebappInstallFailed(WebappInstallFailed),
}
