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
//! - else log + ack as no-op
//!
//! This module is the slice-4-prep skeleton: bodies are log-only and
//! return `Ok(())`. Slice 4 fills the dispatch paths and adds
//! `BluetoothMan` / iAP2 transport-command channel dependencies.
//! Architecture and rationale: `notes/transport-controller.md`.

use libbridgething::RepeatMode;

// methods + error variants below are wired for slice 4's HID + companion-transport dispatch;
// the skeleton bodies log only and the routing code that surfaces NoTarget/StateUnknown lands then.
#[allow(dead_code)]
#[derive(Debug, Clone, Default)]
pub struct TransportController;

#[allow(dead_code)]
impl TransportController {
  pub fn new() -> Self {
    Self
  }

  pub async fn play(&self) -> TransportResult<()> {
    self.stub("play")
  }

  pub async fn pause(&self) -> TransportResult<()> {
    self.stub("pause")
  }

  pub async fn play_pause(&self) -> TransportResult<()> {
    self.stub("play_pause")
  }

  pub async fn next(&self) -> TransportResult<()> {
    self.stub("next")
  }

  pub async fn prev(&self) -> TransportResult<()> {
    self.stub("prev")
  }

  pub async fn volume_up(&self) -> TransportResult<()> {
    self.stub("volume_up")
  }

  pub async fn volume_down(&self) -> TransportResult<()> {
    self.stub("volume_down")
  }

  pub async fn mute_toggle(&self) -> TransportResult<()> {
    self.stub("mute_toggle")
  }

  pub async fn set_shuffle(&self, on: bool) -> TransportResult<()> {
    self.stub(&format!("set_shuffle({on})"))
  }

  pub async fn set_repeat(&self, mode: RepeatMode) -> TransportResult<()> {
    self.stub(&format!("set_repeat({mode:?})"))
  }

  pub async fn seek_to(&self, position_ms: u32) -> TransportResult<()> {
    self.stub(&format!("seek_to({position_ms})"))
  }

  pub async fn skip_to_index(&self, index: u32) -> TransportResult<()> {
    self.stub(&format!("skip_to_index({index})"))
  }

  fn stub(&self, verb: &str) -> TransportResult<()> {
    tracing::info!("transport command {verb}: waiting on slice 4 dispatch wiring");
    Ok(())
  }
}

pub type TransportResult<T> = Result<T, TransportError>;

#[allow(dead_code)]
#[derive(Debug, thiserror::Error)]
pub enum TransportError {
  #[error("no peer with a transport route is connected")]
  NoTarget,
  #[error("iap2 shuffle/repeat state not yet known; refused to toggle")]
  StateUnknown,
}
