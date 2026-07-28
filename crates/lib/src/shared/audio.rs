//! Audio surface errors. Volume, mute, TTS and earcon verbs are all
//! fire-and-forget commands, so a refusal has no reply to ride on and
//! surfaces as an event instead.

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use typeshare::typeshare;

#[typeshare]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(tag = "type", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "shared.ts")]
pub enum AudioError {
  /// The companion accepted the verb but the platform refused it (no output
  /// route, audio session denied, another app holds the channel).
  ActionRejected { reason: String },
  /// Speech synthesis could not start, or the platform cut it short.
  TtsFailed { reason: String },
  /// The verb needs a capability the connected companion does not advertise.
  Unavailable { verb: String },
  /// No companion is connected, so there is nowhere to send the verb.
  NoTarget,
}
