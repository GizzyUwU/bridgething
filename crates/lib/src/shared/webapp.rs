use serde::{Deserialize, Serialize};
use ts_rs::TS;
use typeshare::typeshare;
use uuid::Uuid;

#[typeshare]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub enum WebappSource {
  Builtin,
  Installed,
}

#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub struct WebappInfo {
  #[ts(type = "string")]
  #[typeshare(serialized_as = "Vec<u8>")]
  pub id: Uuid,
  pub name: String,
  pub source: WebappSource,
  pub version: String,
  pub description: Option<String>,
  pub icon_available: bool,
  pub icon_mime: Option<String>,
  pub config: Vec<ConfigField>,
  pub permissions: Vec<String>,
}

/// On-disk `manifest.json` shape. Read from the bundle at install time
/// and validated; the resulting metadata projects to `WebappInfo` for
/// the wire.
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub struct WebappManifest {
  #[ts(type = "string")]
  #[typeshare(serialized_as = "Vec<u8>")]
  pub id: Uuid,
  pub name: String,
  pub version: String,
  pub description: Option<String>,
  pub icon: Option<String>,
  #[serde(default)]
  pub config: Vec<ConfigField>,
  #[serde(default)]
  pub permissions: Vec<String>,
}

/// One declared user-tunable setting. Adjacent-tagged on the wire to
/// stay typeshare-compatible: `{"type":"string","data":{"key":"zip",...}}`.
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[serde(tag = "type", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub enum ConfigField {
  String(StringField),
  Number(NumberField),
  Boolean(BoolField),
  Enum(EnumField),
  /// String semantics, masked in companion UI. No actual secure storage.
  Secret(StringField),
}

#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub struct StringField {
  pub key: String,
  pub label: String,
  pub pattern: Option<String>,
  pub min_length: Option<u32>,
  pub max_length: Option<u32>,
  pub default: Option<String>,
}

#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub struct NumberField {
  pub key: String,
  pub label: String,
  pub min: Option<f64>,
  pub max: Option<f64>,
  pub step: Option<f64>,
  pub default: Option<f64>,
}

#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub struct BoolField {
  pub key: String,
  pub label: String,
  pub default: Option<bool>,
}

#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub struct EnumField {
  pub key: String,
  pub label: String,
  pub choices: Vec<String>,
  pub default: Option<String>,
}

impl ConfigField {
  pub fn key(&self) -> &str {
    match self {
      ConfigField::String(f) | ConfigField::Secret(f) => &f.key,
      ConfigField::Number(f) => &f.key,
      ConfigField::Boolean(f) => &f.key,
      ConfigField::Enum(f) => &f.key,
    }
  }

  pub fn default_as_storage(&self) -> Option<String> {
    match self {
      ConfigField::String(f) | ConfigField::Secret(f) => f.default.clone(),
      ConfigField::Enum(f) => f.default.clone(),
      ConfigField::Number(f) => f.default.map(|n| n.to_string()),
      ConfigField::Boolean(f) => f.default.map(|b| b.to_string()),
    }
  }
}

/// One key/value pair as exposed by config read APIs. `value` is always a
/// string; consumers parse per the field's declared kind (number → parseFloat,
/// boolean → "true"/"false", string/enum/secret → as-is).
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub struct ConfigEntry {
  pub key: String,
  pub value: String,
}
