use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StockVersionSend {
  #[serde(rename = "version_status")]
  Status {
    serial: String,
    os_version: String,
    country: String,
    app_version: String,
    fw_version: String,
    model_name: String,
    fcc_id: String,
    ic_id: String,
    discord: String,
    credits: String,
  },
}
