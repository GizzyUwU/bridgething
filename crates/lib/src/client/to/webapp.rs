use bridgething_macros::BridgeEnum;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::{WebappError, WebappInfo};

#[serde_with::serde_as]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct WebappIconReply {
  #[serde_as(as = "serde_with::Bytes")]
  #[ts(type = "Uint8Array")]
  pub bytes: Vec<u8>,
  pub mime: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct WebappListReply {
  pub webapps: Vec<WebappInfo>,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct WebappCurrentReply {
  #[ts(type = "string | null")]
  pub id: Option<Uuid>,
  pub name: Option<String>,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct WebappActiveReply {
  #[ts(type = "string | null")]
  pub id: Option<Uuid>,
  pub name: Option<String>,
}

/// Successful response to `WebappInstallBegin`. The webapp's next
/// `WebappInstallChunk` should start at `resume_from_offset`; 0 for a
/// fresh push, or the byte count already on disk when resuming.
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct WebappInstallBeginAck {
  pub resume_from_offset: u32,
}

/// Asynchronous failure event for an install whose upload completed
/// but failed verify / extract / validation. Pairs with the
/// `WebappInstalled` event as the terminal-outcome duo.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct WebappInstallFailed {
  pub install_id: String,
  pub error: WebappError,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct WebappActiveChanged {
  pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct WebappUninstalled {
  pub name: String,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS, BridgeEnum)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
#[bridge_enum(into = crate::client::BridgeToClientMsgData)]
pub enum BridgeToClientWebappMsg {
  #[bridge_response]
  ListReply(WebappListReply),
  #[bridge_response]
  CurrentReply(WebappCurrentReply),
  #[bridge_response]
  ActiveReply(WebappActiveReply),
  #[bridge_response]
  IconReply(WebappIconReply),
  /// response to InstallBegin indicating the resume offset for the next chunk
  #[bridge_response]
  InstallBeginAck(WebappInstallBeginAck),
  /// domain-level error response for any webapp op
  #[bridge_response]
  WebappError(WebappError),
  #[bridge_event]
  ActiveChanged(WebappActiveChanged),
  /// event: a chunked install completed successfully (broadcast to all
  /// webapp peers, including the one that initiated).
  #[bridge_event]
  WebappInstalled(WebappInfo),
  /// event: a chunked install failed post-upload (broadcast to all
  /// webapp peers).
  #[bridge_event]
  WebappInstallFailed(WebappInstallFailed),
  #[bridge_event]
  WebappUninstalled(WebappUninstalled),
}
