use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StockSetupSend {
  #[serde(rename = "setup_status")]
  Status { payload: String },
}
