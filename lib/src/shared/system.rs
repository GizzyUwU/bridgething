use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export, export_to = "shared.ts")]
pub enum PhoneCallStatus {
  Disconnected,
  Sending,
  Ringing,
  Connecting,
  Active,
  Held,
  Disconnecting,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export, export_to = "shared.ts")]
pub enum PhoneCallDirection {
  Incoming,
  Outgoing,
}
