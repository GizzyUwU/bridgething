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
/// Reply to the `icon` request: the raw icon bytes declared by the webapp's manifest.
pub struct WebappIconReply {
  #[serde_as(as = "serde_with::Bytes")]
  #[ts(type = "Uint8Array")]
  pub bytes: Vec<u8>,
  /// MIME type declared by the manifest's icon; `None` if the manifest didn't declare one.
  pub mime: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct WebappListReply {
  /// Excludes `Launcher`-role bundles used as alternate home screens.
  pub webapps: Vec<WebappInfo>,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct WebappCurrentReply {
  /// `None` when no webapp is currently active in the kiosk.
  #[ts(type = "string | null")]
  pub id: Option<Uuid>,
  pub name: Option<String>,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct WebappActiveReply {
  /// Id of the webapp that was just activated.
  #[ts(type = "string | null")]
  pub id: Option<Uuid>,
  pub name: Option<String>,
}

/// Broadcast when the active webapp changes; carries the new active webapp's name.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "client.ts")]
pub struct WebappActiveChanged {
  pub name: String,
}

/// Broadcast when an installed webapp is removed; carries the removed webapp's name.
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
/// Daemon -> webapp replies and events for the webapp-management surface. `ListReply`,
/// `CurrentReply`, `ActiveReply`, and `IconReply` answer the matching `ClientToBridgeWebappMsg`
/// request; `WebappError` replaces the reply on failure. `WebappInstalled` is a live event
/// broadcast to every connected webapp whenever an installed-webapp transfer completes.
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
