use derive_more::derive::Debug;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use typeshare::typeshare;
use uuid::Uuid;

/// Domain errors emitted by any webapp surface (gateway- or client-side).
/// Single catalog: both protocols speak the same variant set.
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(tag = "type", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub enum WebappError {
  /// No installed webapp matches this id (uninstall / activate / icon / config target).
  WebappNotFound { id: String },
  /// Built-in webapps cannot be uninstalled.
  CannotUninstallBuiltin { id: String },
  /// Install rejected: the manifest's id is in the reserved-uuid set
  /// (stock, hub, launcher, etc).
  IdReserved { id: String },
  /// Extracted bundle exceeds the 1 GiB disk-protection cap.
  ExtractedTooLarge { max_bytes: u32 },
  /// Zip extraction failed: corrupt archive, unsafe entry names, etc.
  ZipMalformed { reason: String },
  /// Bundle has no index.html at its root.
  MissingIndexHtml,
  /// manifest.json missing, unparseable, or failed schema validation.
  InvalidManifest { reason: String },
  /// The webapp's manifest doesn't declare an icon (or the icon file is missing on disk).
  IconNotAvailable { id: String },
  /// Config key is not declared in the webapp's manifest schema.
  UnknownConfigKey { key: String },
  /// Value failed schema validation (out of range, regex mismatch, not in enum).
  InvalidConfigValue { key: String, reason: String },
  /// Catch-all for genuinely-unexpected failures (io errors, daemon-side
  /// bugs). Reason is human-readable; not a stable wire contract.
  Internal { reason: String },
}

#[typeshare]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub enum WebappSource {
  Builtin,
  Installed,
}

/// A webapp's launcher visibility. `Standard` shows up in user-facing
/// listings (the hub grid, etc); `Launcher` is itself a launcher and is
/// hidden from those listings. The daemon filters `Launcher` bundles
/// out of `client.webapp.list`; the gateway list keeps everything.
#[typeshare]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub enum WebappRole {
  #[default]
  Standard,
  Launcher,
}

/// Art render sizes a webapp declares so the companion warms exactly the
/// pixels it renders: hero (now-playing / detail views) and thumb (queue /
/// grid). Omitted in a manifest falls back to the canonical `{248, 96}`,
/// which is also the stock webapp's profile.
#[typeshare]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub struct ArtProfile {
  pub hero_px: u32,
  pub thumb_px: u32,
}

impl Default for ArtProfile {
  fn default() -> Self {
    Self { hero_px: 248, thumb_px: 96 }
  }
}

#[typeshare]
#[serde_with::serde_as]
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
  pub role: WebappRole,
  pub version: String,
  pub description: Option<String>,
  pub icon_available: bool,
  pub icon_mime: Option<String>,
  /// icon bytes, inlined on the gateway list so the companion never round-trips
  /// a separate fetch per app. omitted on the on-device client list.
  #[debug(skip)]
  #[serde_as(as = "Option<serde_with::Bytes>")]
  #[ts(type = "Uint8Array | null")]
  #[typeshare(serialized_as = "Option<Vec<u8>>")]
  pub icon: Option<Vec<u8>>,
  pub config: Vec<ConfigField>,
  pub permissions: Vec<String>,
  /// Plain-English description of the voice intents the webapp wants
  /// WEBAPP_INTENT routing for. Companion-side NLU folds this into the
  /// "currently active extensions" section of the system prompt at
  /// inference, which is what makes WEBAPP_INTENT emission context-aware.
  /// `None` opts the webapp out of voice integration.
  pub voice_grammar: Option<String>,
  /// Declared art render sizes; the companion warms exactly these. `None`
  /// means the canonical `{248, 96}` default applies.
  pub art: Option<ArtProfile>,
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
  pub role: WebappRole,
  #[serde(default)]
  pub config: Vec<ConfigField>,
  #[serde(default)]
  pub permissions: Vec<String>,
  /// Optional plain-English description of the voice commands this
  /// webapp wants WEBAPP_INTENT routing for. The companion's NLU folds
  /// the grammars of all installed-and-active webapps into the system
  /// prompt at inference. Webapps that don't declare a grammar opt out
  /// of voice integration.
  pub voice_grammar: Option<String>,
  /// Declared art render sizes. Omitted falls back to `{248, 96}`.
  #[serde(default)]
  pub art: Option<ArtProfile>,
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
/// string; consumers parse per the field's declared kind (number -> parseFloat,
/// boolean -> "true"/"false", string/enum/secret -> as-is).
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub struct ConfigEntry {
  pub key: String,
  pub value: String,
}
