use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum StockDeviceRecv {
  Reboot,
  PowerOff,
  FactoryReset,
  ReturnToSpotify,

  PhoneCallAnswer { attributes: PhoneCallAttributes },
  PhoneCallEnd { attributes: PhoneCallAttributes },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PhoneCallAttributes {
  call_id: String,
}
