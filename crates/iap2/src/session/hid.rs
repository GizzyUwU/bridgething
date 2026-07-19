//! HID flow: declares the accessory's virtual HID device once
//! identification reaches Accepted, then translates inbound transport
//! commands into press/release `AccessoryHIDReport` pairs on the iAP2
//! control session.
//!
//! Bridgething uses HID exclusively as the **outbound** path for media
//! intents when the companion has not claimed `NowPlayingPlayback`
//! authority. Hardware events (wheel rotation, presets, back/settings)
//! are captured by the on-device webapp; HID never carries them.
//!
//! Each transport tap fires two `AccessoryHIDReport`s back-to-back: a
//! press frame with the chosen bit(s) set, then a release frame with all
//! bits cleared. iOS treats a missing release as a held button. The
//! release is delayed by [`TAP_RELEASE_DELAY`] to give iOS a clean edge.

use std::time::Duration;

use bytes::Bytes;
use tokio::sync::mpsc;

use super::{SessionEvent, send_csm};
use crate::{
  csm::{
    CsmFrame,
    hid::{
      DeviceHIDReport, HIDComponentUpdate, PRODUCT_ID, StartHID, StartNativeHID, StopHID, TRANSPORT_COMPONENT_ID,
      TRANSPORT_DESCRIPTOR, VENDOR_ID, transport_report,
    },
  },
  error::Result,
  link::Iap2Command,
};

const TAP_RELEASE_DELAY: Duration = Duration::from_millis(10);
const INTER_TAP_DELAY: Duration = Duration::from_millis(60);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HidCommand {
  Pulse(u8),
  Sequence { mask: u8, count: u8 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HidState {
  Idle,
  Started,
  Stopped,
}

pub(super) struct HidFlow {
  state: HidState,
  rx: mpsc::Receiver<HidCommand>,
}

impl HidFlow {
  pub(super) fn new(rx: mpsc::Receiver<HidCommand>) -> Self {
    Self {
      state: HidState::Idle,
      rx,
    }
  }

  pub(super) fn handles(msg_id: u16) -> bool {
    matches!(msg_id, 0x6801 | 0x6806 | 0x6807)
  }

  pub(super) async fn handle(
    &mut self,
    frame: CsmFrame,
    _session_events_tx: &mpsc::Sender<SessionEvent>,
  ) -> Result<Option<SessionEvent>> {
    match frame.msg_id {
      0x6801 => {
        let report = DeviceHIDReport::try_from(frame)?;
        tracing::trace!(
          component_id = report.component_id,
          bytes = report.report.len(),
          "iap2 hid: inbound DeviceHIDReport (logged, not dispatched)"
        );
      }
      0x6806 => {
        let _ = StartNativeHID::try_from(frame)?;
        tracing::debug!("iap2 hid: inbound StartNativeHID");
      }
      0x6807 => {
        let update = HIDComponentUpdate::try_from(frame)?;
        tracing::debug!(
          component_id = update.component_id,
          enabled = update.component_enabled,
          "iap2 hid: inbound HIDComponentUpdate"
        );
      }
      _ => {}
    }
    Ok(None)
  }

  pub(super) async fn ensure_started(&mut self, link_command_tx: &mpsc::Sender<Iap2Command>) -> Result<()> {
    if matches!(self.state, HidState::Idle) {
      tracing::debug!("iap2 hid: sending StartHID");
      let start = StartHID {
        component_id: TRANSPORT_COMPONENT_ID,
        vendor_id: VENDOR_ID,
        product_id: PRODUCT_ID,
        descriptor: Bytes::from_static(TRANSPORT_DESCRIPTOR),
      };
      send_csm(start, link_command_tx).await?;
      self.state = HidState::Started;
    }
    Ok(())
  }

  pub(super) fn reset(&mut self) {
    self.state = HidState::Idle;
  }

  pub(super) async fn recv(&mut self) -> Option<HidCommand> {
    self.rx.recv().await
  }

  pub(super) async fn handle_command(
    &mut self,
    cmd: HidCommand,
    link_command_tx: &mpsc::Sender<Iap2Command>,
  ) -> Result<()> {
    if !matches!(self.state, HidState::Started) {
      tracing::warn!(?cmd, state = ?self.state, "iap2 hid: command before StartHID; dropping");
      return Ok(());
    }

    match cmd {
      HidCommand::Pulse(0) => {
        tracing::trace!("iap2 hid: ignoring zero-mask pulse");
        Ok(())
      }
      HidCommand::Pulse(mask) => self.send_pulse(mask, link_command_tx).await,
      HidCommand::Sequence { mask: 0, .. } => {
        tracing::trace!("iap2 hid: ignoring zero-mask sequence");
        Ok(())
      }
      HidCommand::Sequence { mask, count } => {
        for i in 0..count {
          self.send_pulse(mask, link_command_tx).await?;
          if i + 1 < count {
            tokio::time::sleep(INTER_TAP_DELAY).await;
          }
        }
        Ok(())
      }
    }
  }

  pub(super) async fn shutdown(&mut self, link_command_tx: &mpsc::Sender<Iap2Command>) -> Result<()> {
    if matches!(self.state, HidState::Started) {
      let stop = StopHID {
        component_id: TRANSPORT_COMPONENT_ID,
      };
      send_csm(stop, link_command_tx).await?;
    }
    self.state = HidState::Stopped;
    Ok(())
  }

  async fn send_pulse(&self, mask: u8, link_command_tx: &mpsc::Sender<Iap2Command>) -> Result<()> {
    tracing::trace!(mask = format!("{mask:#04x}"), "iap2 hid: pulse");
    send_csm(transport_report(mask), link_command_tx).await?;
    tokio::time::sleep(TAP_RELEASE_DELAY).await;
    send_csm(transport_report(0), link_command_tx).await?;
    Ok(())
  }
}
