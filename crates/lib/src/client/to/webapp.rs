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
  /// domain-level error response for any webapp op
  #[bridge_response]
  WebappError(WebappError),
  #[bridge_event]
  ActiveChanged(WebappActiveChanged),
  /// event: a webapp install completed successfully
  #[bridge_event]
  WebappInstalled(WebappInfo),
  #[bridge_event]
  WebappUninstalled(WebappUninstalled),
}
