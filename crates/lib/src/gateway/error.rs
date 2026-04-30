use serde::{Deserialize, Serialize};
use ts_rs::TS;
use typeshare::typeshare;

/// Protocol-level failure that the bridge ships when a request could not be
/// reached or dispatched. Carried by `BridgeToGatewayMsgData::Error`.
///
/// Domain-level errors (predictable, op-specific failures the caller may want
/// to recover from) live inside the per-op response variant — for example
/// `BridgeToGatewayWebappMsg::WebappError(WebappError)`.
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(tag = "type", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "gateway.ts")]
pub enum GatewayError {
  /// The bridge does not implement this request variant.
  Unsupported,
  /// The bridge could not decode or validate the request payload.
  Malformed { reason: String },
  /// An unexpected internal error occurred while handling the request.
  HandlerFailed { reason: String },
}
