//! Capabilities surface — single broadcast struct describing what the
//! currently-connected companion (if any) can do, plus the daemon's
//! authority view. The companion announces `GatewayCapabilities` at
//! session-up; the daemon merges that with its own bits and re-emits
//! `Capabilities` to webapps. `gateway: None` means no companion is
//! connected and most surfaces are inert.

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use typeshare::typeshare;

use super::CompanionAuthorityScope;

/// Identity payload describing the companion peer the daemon is talking
/// to. Replaces the old `GatewayMeta` and the old `GatewayStatus` bits.
/// Address is the BT MAC the companion advertises; on transports without
/// a stable MAC (the network gateway's `0xfe:fe:...` synthetic addrs) it
/// is the synthetic address as a string.
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub struct GatewayInfo {
  pub address: String,
  pub name: String,
  pub os_name: String,
  pub app_name: String,
  pub app_version: String,
  pub adapter_version: String,
  pub lib_version: String,
  pub libbridgething_version: String,
}

#[typeshare]
#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub enum NetworkKind {
  #[default]
  Unknown,
  Wifi,
  Cellular,
  Ethernet,
}

/// What kind of network the companion's host is currently using. `metered`
/// is the OS-reported metered flag; webapps should treat it as a hint
/// (e.g. defer non-essential fetches) rather than a hard ban.
#[typeshare]
#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub struct NetworkInfo {
  pub kind: NetworkKind,
  pub metered: bool,
}

/// Bool feature flags the daemon exposes to webapps. Each is true when the
/// surface has both a backing implementation and (where applicable) a
/// connected companion claiming to provide it. False = the surface will
/// respond `Unsupported` or `Unimplemented` to verbs.
#[typeshare]
#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub struct SurfaceAvailability {
  pub geo: bool,
  pub notifications: bool,
  pub net_fetch: bool,
  pub net_ws: bool,
  pub audio_tts: bool,
  pub lyrics: bool,
}

/// Which music service the companion is currently logged into and
/// driving on behalf of the user. `None` when no glue is attached.
#[typeshare]
#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub enum MusicProvider {
  #[default]
  None,
  Spotify,
  AppleMusic,
  Tidal,
}

/// One TTS voice the companion's audio backend can speak as. `id` is
/// platform-opaque (Apple/Android voice id); `locale` is BCP-47.
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub struct VoiceDescriptor {
  pub id: String,
  pub name: String,
  pub locale: String,
}

/// What the gateway-side audio backend supports. `earcons` are short
/// named sounds the companion can play; `voices` are TTS voices.
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub struct AudioCapabilities {
  pub earcons: Vec<String>,
  pub voices: Vec<VoiceDescriptor>,
}

/// What a connected companion advertises about itself. Sent on every
/// session-up via `GatewayToBridgeCapabilitiesMsg::Announce`, and re-sent
/// on any change. The daemon caches the latest snapshot per peer and
/// derives the webapp-facing `Capabilities` from it.
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub struct GatewayCapabilities {
  pub gateway: GatewayInfo,
  pub uri_schemes: Vec<String>,
  pub network: NetworkInfo,
  pub available: SurfaceAvailability,
  pub audio: AudioCapabilities,
  pub music_provider: MusicProvider,
}

/// What the daemon advertises to webapps. `gateway: None` means no
/// companion is connected; webapps that depend on companion-routed
/// surfaces (Library, Net, Notifications, etc.) should branch on this.
/// `authority` is the live set of scopes the companion currently claims.
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub struct Capabilities {
  pub gateway: Option<GatewayInfo>,
  pub available: SurfaceAvailability,
  pub authority: Vec<CompanionAuthorityScope>,
  pub uri_schemes: Vec<String>,
  pub network: NetworkInfo,
  pub audio: AudioCapabilities,
  pub music_provider: MusicProvider,
}
