use serde::{Deserialize, Serialize};
use ts_rs::TS;
use typeshare::typeshare;

/// Closed intent enum the companion-side NLU pipeline emits. The full
/// catalog (47 intents) lives in `notes/voice/intent-schema.md` and is
/// also encoded in `configs/grammar.strict.json` (the json_schema the
/// LLM is decoded against). At the wire boundary we serialize as a
/// string for forward-compat; the daemon dispatcher matches on the
/// well-known SHOUTY_SNAKE values.
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub struct NluConfidence {
  /// "low" | "medium" | "high". The LLM emits one of these per channel.
  pub intent: String,
  pub slots: Option<String>,
}

/// One alternate interpretation the LLM surfaced alongside the primary.
/// Populated when the LLM returns `ambiguous_alternates` in its
/// json_schema output; consumed by the companion's CLARIFY UI so the
/// user can pick.
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub struct NluAlternate {
  pub intent: String,
  pub slots: Option<NluSlots>,
}

/// Slot catalog. Every variant in `intents.yaml` projects through this
/// flat shape; per-intent slot allowlists are enforced by the
/// json_schema grammar at decode time, not by this struct. The wire
/// payload omits absent slots (`#[serde_with::skip_serializing_none]`)
/// so a PLAY-with-artist row is just `{ "artist": "..." }` on the wire.
///
/// String values are passed through verbatim from the user's transcript
/// (no normalization at this layer); the SpotifyResolver may decorate
/// the slots with a `uri` after catalog lookup.
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub struct NluSlots {
  pub artist: Option<String>,
  pub track: Option<String>,
  pub album: Option<String>,
  pub playlist: Option<String>,
  pub podcast: Option<String>,
  pub episode: Option<String>,
  pub mood: Option<String>,
  pub genre: Option<String>,
  pub era: Option<String>,
  pub popularity_filter: Option<String>,
  pub entity_type: Option<String>,
  pub query: Option<String>,
  /// WEBAPP_INTENT only: the filler-stripped natural-language command
  /// the active webapp's voice grammar handler will parse.
  pub raw_query: Option<String>,
  /// WEBAPP_INTENT / OPEN_WEBAPP only.
  pub webapp_id: Option<String>,
  pub webapp_name: Option<String>,
  /// PLAY_PRESET / SAVE_TO_PRESET only. String because users say "two"
  /// but stock SLIMO expects an Array<string> envelope - the stock
  /// translation wraps as needed.
  pub preset: Option<String>,
  /// VOLUME_UP / VOLUME_DOWN. "small" | "large" | numeric step.
  pub amount: Option<String>,
  /// VOLUME_ABSOLUTE. 0-100.
  pub level: Option<u32>,
  /// Post-resolution Spotify URI. Populated by the companion's
  /// SpotifyResolver after the NLU stage; daemon dispatches directly
  /// to playback when set.
  pub uri: Option<String>,
}

/// What the companion-side NLU resolved an utterance to, sent across
/// the gateway link for the daemon to dispatch. This is the bridgething-
/// native shape; the daemon's stock-compat layer wraps it into the
/// SLIMO `NluMessage` envelope when the active webapp is stock.
///
/// `transcript` is the ASR output the NLU ran on; carried so the daemon
/// can echo it for telemetry and so SHOW+UNKNOWN+query="DJ"-style
/// stock-compat fallbacks have the raw query available.
#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub struct NluResolvedIntent {
  pub intent: String,
  #[serde(default)]
  pub slots: NluSlots,
  pub transcript: String,
  pub confidence: Option<NluConfidence>,
  pub alternates: Option<Vec<NluAlternate>>,
}

/// Why dispatch declined to act on a `VoiceDispatch`. The companion
/// surfaces these to the user (toast / UI hint); the daemon does not
/// otherwise retain state about the failure.
#[typeshare]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub enum VoiceDispatchErrorCode {
  /// WEBAPP_INTENT targeted a webapp_id that isn't installed.
  WebappNotInstalled,
  /// WEBAPP_INTENT targeted an installed webapp that isn't the active
  /// one. Companion can prompt the user to switch.
  WebappNotActive,
  /// Active webapp accepted the dispatch but reported an error.
  WebappRefused,
  /// Intent is CLARIFY / NO_INTENT - companion should resolve at its
  /// own edge rather than asking the daemon to dispatch.
  NotDispatchable,
  /// Stock playback target couldn't be resolved (no Spotify session,
  /// missing slot, etc.).
  PlaybackFailed,
  /// Catch-all (io error, internal state machine glitch).
  Internal,
}

/// Where the daemon actually routed a successful dispatch. Carried back
/// to the companion so it can render the right confirmation UI.
#[typeshare]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub enum VoiceDispatchTarget {
  /// Stock playback path (PLAY/PAUSE/NEXT/etc) - translated into the
  /// SLIMO `NluMessage` and handed to the stock webapp.
  StockPlayback,
  /// Forwarded to the active webapp's voice handler.
  ActiveWebapp,
  /// Switched the active webapp via OPEN_WEBAPP.
  WebappSwitch,
}
