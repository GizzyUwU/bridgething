use libbridgething::{Device, DeviceType};
use sea_orm::entity::prelude::*;

/// Persistent record of a peer the daemon has ever talked to. The
/// in-memory `PeerTracker` derives its working set on every connect /
/// disconnect; this table is the slow-path, cross-boot ledger.
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "devices")]
pub struct Model {
  #[sea_orm(primary_key, auto_increment = false)]
  pub mac: String,
  pub name: String,
  pub device_type: String,
  pub is_default: bool,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

impl From<&Model> for Device {
  fn from(m: &Model) -> Self {
    Device {
      name: m.name.clone(),
      device_type: parse_device_type(&m.device_type),
      mac: m.mac.clone(),
      default: m.is_default,
    }
  }
}

impl Model {
  pub fn from_wire(d: &Device) -> Self {
    Self {
      mac: d.mac.clone(),
      name: d.name.clone(),
      device_type: device_type_str(&d.device_type).to_string(),
      is_default: d.default,
    }
  }
}

fn device_type_str(t: &DeviceType) -> &'static str {
  match t {
    DeviceType::Android => "android",
    DeviceType::Ios => "ios",
    DeviceType::Windows => "windows",
    DeviceType::MacOS => "macos",
    DeviceType::Linux => "linux",
    DeviceType::Unknown => "unknown",
  }
}

fn parse_device_type(s: &str) -> DeviceType {
  match s {
    "android" => DeviceType::Android,
    "ios" => DeviceType::Ios,
    "windows" => DeviceType::Windows,
    "macos" => DeviceType::MacOS,
    "linux" => DeviceType::Linux,
    _ => DeviceType::Unknown,
  }
}
