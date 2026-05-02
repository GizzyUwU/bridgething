//! Outbound media-control multiplexer.
//!
//! Webapp tap on a transport verb (play, pause, next, prev, shuffle,
//! repeat, volume up/down, mute, seek, skip-to-index) arrives at the
//! daemon's local websocket, dispatches through
//! [`crate::handler::client::interaction::InteractionHandler`], and
//! lands here. The controller decides where the command goes based on
//! `AuthorityRegistry`:
//!
//! - companion authoritative for `NowPlayingPlayback` -> typed
//!   `BridgeToGatewayTransportMsg` over the gateway link
//! - else iAP2 control session (when an iPhone is identified) ->
//!   `AccessoryHIDReport` on Consumer Control page 0x0C
//! - else no target; warn-log and ack
//!
//! Shuffle and repeat over HID are toggle-only on the iAP2 wire. To
//! honour state-set semantics the controller reads the latest
//! `iap2_playback` snapshot from `Player` and computes how many toggles
//! to fire (0/1 for shuffle, 0/1/2 for repeat). When the snapshot lacks
//! the field the controller refuses with a warn log and returns `Ok(())`
//! so the webapp's UI doesn't error-spinner.
//!
//! Architecture and rationale: `notes/transport-controller.md`.

use bridgething_iap2::HidCommand;
use libbridgething::{
  RepeatMode,
  gateway::{
    BridgeToGatewayTransportMsgCommand, CompanionAuthorityScope, RepeatSet, SeekToSet, ShuffleSet, SkipToIndexSet,
  },
};

use crate::{
  authority::AuthorityRegistry,
  bluetooth::{BluetoothMan, iap2::Iap2TransportHandle},
  player::Player,
};

/// Bit positions inside the bridgething HID transport descriptor. Mirrored
/// from `bridgething_iap2::csm::hid::report_bit` so the controller can
/// build pulse masks without depending on the iap2 crate's bytes-level
/// types directly.
mod hid_bit {
  pub const PLAY_PAUSE: u8 = 0x01;
  pub const NEXT: u8 = 0x02;
  pub const PREV: u8 = 0x04;
  pub const VOLUME_UP: u8 = 0x08;
  pub const VOLUME_DOWN: u8 = 0x10;
  pub const MUTE: u8 = 0x20;
  pub const SHUFFLE: u8 = 0x40;
  pub const REPEAT: u8 = 0x80;
}

#[derive(Debug, Clone)]
pub struct TransportController {
  authority: AuthorityRegistry,
  player: Player,
  bluetooth: BluetoothMan,
  iap2: Option<Iap2TransportHandle>,
}

impl TransportController {
  pub fn new(
    authority: AuthorityRegistry,
    player: Player,
    bluetooth: BluetoothMan,
    iap2: Option<Iap2TransportHandle>,
  ) -> Self {
    Self {
      authority,
      player,
      bluetooth,
      iap2,
    }
  }

  pub async fn play(&self) -> TransportResult<()> {
    self
      .dispatch_simple("play", BridgeToGatewayTransportMsgCommand::Play, hid_bit::PLAY_PAUSE)
      .await
  }

  pub async fn pause(&self) -> TransportResult<()> {
    self
      .dispatch_simple("pause", BridgeToGatewayTransportMsgCommand::Pause, hid_bit::PLAY_PAUSE)
      .await
  }

  pub async fn play_pause(&self) -> TransportResult<()> {
    self
      .dispatch_simple(
        "play_pause",
        BridgeToGatewayTransportMsgCommand::PlayPause,
        hid_bit::PLAY_PAUSE,
      )
      .await
  }

  pub async fn next(&self) -> TransportResult<()> {
    self
      .dispatch_simple("next", BridgeToGatewayTransportMsgCommand::Next, hid_bit::NEXT)
      .await
  }

  pub async fn prev(&self) -> TransportResult<()> {
    self
      .dispatch_simple("prev", BridgeToGatewayTransportMsgCommand::Prev, hid_bit::PREV)
      .await
  }

  pub async fn volume_up(&self) -> TransportResult<()> {
    self
      .dispatch_simple(
        "volume_up",
        BridgeToGatewayTransportMsgCommand::VolumeUp,
        hid_bit::VOLUME_UP,
      )
      .await
  }

  pub async fn volume_down(&self) -> TransportResult<()> {
    self
      .dispatch_simple(
        "volume_down",
        BridgeToGatewayTransportMsgCommand::VolumeDown,
        hid_bit::VOLUME_DOWN,
      )
      .await
  }

  pub async fn mute_toggle(&self) -> TransportResult<()> {
    self
      .dispatch_simple(
        "mute_toggle",
        BridgeToGatewayTransportMsgCommand::MuteToggle,
        hid_bit::MUTE,
      )
      .await
  }

  pub async fn set_shuffle(&self, on: bool) -> TransportResult<()> {
    if self.companion_owns_playback() {
      tracing::debug!("transport set_shuffle({on}): routing to companion");
      return self
        .send_companion(BridgeToGatewayTransportMsgCommand::Shuffle(ShuffleSet { on }))
        .await;
    }
    let snapshot = self.player.iap2_playback_snapshot().await;
    match snapshot.shuffle {
      None => {
        tracing::warn!("transport set_shuffle({on}): iap2 shuffle state unknown; refusing toggle");
        Ok(())
      }
      Some(current) if current == on => {
        tracing::debug!("transport set_shuffle({on}): already in target state");
        Ok(())
      }
      Some(_) => {
        tracing::debug!("transport set_shuffle({on}): firing one HID toggle");
        self.send_iap2(HidCommand::Pulse(hid_bit::SHUFFLE)).await
      }
    }
  }

  pub async fn set_repeat(&self, mode: RepeatMode) -> TransportResult<()> {
    if self.companion_owns_playback() {
      tracing::debug!("transport set_repeat({mode:?}): routing to companion");
      return self
        .send_companion(BridgeToGatewayTransportMsgCommand::Repeat(RepeatSet { mode }))
        .await;
    }
    let snapshot = self.player.iap2_playback_snapshot().await;
    match snapshot.repeat {
      None => {
        tracing::warn!("transport set_repeat({mode:?}): iap2 repeat state unknown; refusing toggle");
        Ok(())
      }
      Some(current) if current == mode => {
        tracing::debug!("transport set_repeat({mode:?}): already in target state");
        Ok(())
      }
      Some(current) => {
        let count = repeat_toggle_count(current, mode);
        tracing::debug!("transport set_repeat({mode:?}): firing {count} HID toggle(s)");
        self
          .send_iap2(HidCommand::Sequence {
            mask: hid_bit::REPEAT,
            count,
          })
          .await
      }
    }
  }

  pub async fn seek_to(&self, position_ms: u32) -> TransportResult<()> {
    if self.companion_owns_playback() {
      tracing::debug!("transport seek_to({position_ms}): routing to companion");
      return self
        .send_companion(BridgeToGatewayTransportMsgCommand::SeekTo(SeekToSet { position_ms }))
        .await;
    }
    tracing::warn!("transport seek_to({position_ms}): no iAP2 HID equivalent; ignoring");
    Ok(())
  }

  pub async fn skip_to_index(&self, index: u32) -> TransportResult<()> {
    if self.companion_owns_playback() {
      tracing::debug!("transport skip_to_index({index}): routing to companion");
      return self
        .send_companion(BridgeToGatewayTransportMsgCommand::SkipToIndex(SkipToIndexSet {
          index,
        }))
        .await;
    }
    tracing::warn!("transport skip_to_index({index}): no iAP2 HID equivalent; ignoring");
    Ok(())
  }

  fn companion_owns_playback(&self) -> bool {
    self
      .authority
      .is_authoritative(CompanionAuthorityScope::NowPlayingPlayback)
  }

  /// Common path for verbs that map cleanly to both a companion gateway
  /// variant and a single HID press. Companion path when the companion
  /// holds playback authority; iAP2 HID otherwise.
  async fn dispatch_simple(
    &self,
    verb: &str,
    companion_msg: BridgeToGatewayTransportMsgCommand,
    hid_mask: u8,
  ) -> TransportResult<()> {
    if self.companion_owns_playback() {
      tracing::debug!("transport {verb}: routing to companion");
      return self.send_companion(companion_msg).await;
    }
    tracing::debug!("transport {verb}: routing to iAP2 HID");
    self.send_iap2(HidCommand::Pulse(hid_mask)).await
  }

  async fn send_companion(&self, msg: BridgeToGatewayTransportMsgCommand) -> TransportResult<()> {
    self.bluetooth.gateway_man.broadcast_command(msg).await;
    Ok(())
  }

  async fn send_iap2(&self, cmd: HidCommand) -> TransportResult<()> {
    let Some(handle) = &self.iap2 else {
      tracing::debug!(
        ?cmd,
        "iap2 transport not available (MFi probe failed); dropping HID command"
      );
      return Err(TransportError::NoTarget);
    };
    handle.send(cmd).await;
    Ok(())
  }
}

/// Number of `Repeat` HID toggles to fire to reach `target` from
/// `current`, given iOS Music's Off -> All -> One -> Off cycle.
fn repeat_toggle_count(current: RepeatMode, target: RepeatMode) -> u8 {
  let cur = repeat_cycle_index(current);
  let tgt = repeat_cycle_index(target);
  ((tgt + 3 - cur) % 3) as u8
}

fn repeat_cycle_index(mode: RepeatMode) -> u32 {
  match mode {
    RepeatMode::Off => 0,
    RepeatMode::All => 1,
    RepeatMode::One => 2,
  }
}

pub type TransportResult<T> = Result<T, TransportError>;

#[derive(Debug, thiserror::Error)]
pub enum TransportError {
  #[error("no peer with a transport route is connected")]
  NoTarget,
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn repeat_toggle_count_off_to_all() {
    assert_eq!(repeat_toggle_count(RepeatMode::Off, RepeatMode::All), 1);
  }

  #[test]
  fn repeat_toggle_count_off_to_one() {
    assert_eq!(repeat_toggle_count(RepeatMode::Off, RepeatMode::One), 2);
  }

  #[test]
  fn repeat_toggle_count_all_to_one() {
    assert_eq!(repeat_toggle_count(RepeatMode::All, RepeatMode::One), 1);
  }

  #[test]
  fn repeat_toggle_count_one_to_off() {
    assert_eq!(repeat_toggle_count(RepeatMode::One, RepeatMode::Off), 1);
  }

  #[test]
  fn repeat_toggle_count_one_to_all() {
    assert_eq!(repeat_toggle_count(RepeatMode::One, RepeatMode::All), 2);
  }

  #[test]
  fn repeat_toggle_count_all_to_off() {
    assert_eq!(repeat_toggle_count(RepeatMode::All, RepeatMode::Off), 2);
  }

  #[test]
  fn repeat_toggle_count_same_state_is_zero() {
    assert_eq!(repeat_toggle_count(RepeatMode::Off, RepeatMode::Off), 0);
    assert_eq!(repeat_toggle_count(RepeatMode::All, RepeatMode::All), 0);
    assert_eq!(repeat_toggle_count(RepeatMode::One, RepeatMode::One), 0);
  }
}
