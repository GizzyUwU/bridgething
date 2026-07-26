use derive_more::derive::Debug;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use typeshare::typeshare;
use uuid::Uuid;

/// Upper bound on `WebappInfo::provenance`
pub const WEBAPP_PROVENANCE_MAX_LEN: usize = 2048;

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
  /// Install carried a provenance string over `WEBAPP_PROVENANCE_MAX_LEN`.
  ProvenanceTooLong { max_bytes: u32 },
  /// Zip extraction failed: corrupt archive, unsafe entry names, etc.
  ZipMalformed { reason: String },
  /// Bundle has no index.html at its root.
  MissingIndexHtml,
  /// manifest.json missing, unparseable, or failed schema validation.
  InvalidManifest { reason: String },
  /// The requested resource (icon / settings page) isn't declared by the
  /// webapp's manifest or its file is missing on disk.
  ResourceNotAvailable { id: String },
  /// Config key is not declared in the webapp's manifest schema.
  UnknownConfigKey { key: String },
  /// Value failed schema validation (out of range, regex mismatch, not in enum).
  InvalidConfigValue { key: String, reason: String },
  /// Doc value rejected (oversized).
  InvalidDocValue { key: String, reason: String },
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

/// Which system overlays the daemon injects into the active webapp's page.
/// Every surface defaults to on, so a minimal webapp gets call / pairing /
/// notification / connection / volume UI for free; a full-service webapp
/// declares off the surfaces it draws itself. The wire events the webapp
/// receives are unchanged either way; this only gates the injected UI.
#[typeshare]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub struct OverlayProfile {
  /// Notification toasts.
  #[serde(default = "overlay_surface_default")]
  pub notifications: bool,
  /// Incoming / active call banner.
  #[serde(default = "overlay_surface_default")]
  pub call: bool,
  /// Bluetooth pairing PIN modal.
  #[serde(default = "overlay_surface_default")]
  pub pairing: bool,
  /// Companion-disconnected banner, shown only while a paired phone has
  /// no useful link.
  #[serde(default = "overlay_surface_default")]
  pub connection: bool,
  /// Transient volume level indicator.
  #[serde(default = "overlay_surface_default")]
  pub volume: bool,
}

fn overlay_surface_default() -> bool {
  true
}

impl Default for OverlayProfile {
  fn default() -> Self {
    Self {
      notifications: true,
      call: true,
      pairing: true,
      connection: true,
      volume: true,
    }
  }
}

impl OverlayProfile {
  pub fn any_enabled(&self) -> bool {
    self.notifications || self.call || self.pairing || self.connection || self.volume
  }
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
    Self {
      hero_px: 248,
      thumb_px: 96,
    }
  }
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
  pub role: WebappRole,
  pub version: String,
  pub description: Option<String>,
  pub icon_hash: Option<String>,
  pub settings_hash: Option<String>,
  pub config: Vec<ConfigField>,
  pub permissions: Vec<String>,
  pub renders_voice_display: bool,
  pub art: Option<ArtProfile>,
  pub provenance: Option<String>,
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
  pub settings: Option<String>,
  #[serde(default)]
  pub role: WebappRole,
  #[serde(default)]
  pub config: Vec<ConfigField>,
  #[serde(default)]
  pub permissions: Vec<String>,
  #[serde(default)]
  pub renders_voice_display: bool,
  #[serde(default)]
  pub art: Option<ArtProfile>,
  #[serde(default)]
  pub overlays: OverlayProfile,
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

/// One key/value pair from a webapp's doc namespace: shared structured
/// state writable from both the companion (gateway) and the webapp
/// itself, last write wins. Values are strings; apps encode JSON as
/// needed.
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub struct DocEntry {
  pub key: String,
  pub value: String,
}
