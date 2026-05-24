//! Typed CSMs for the iAP2 HID surface.
//!
//! Bridgething uses HID over iAP2 as the **outbound transport** for media
//! intents (play/pause/next/prev/volume/mute/shuffle/repeat) when the
//! companion has not claimed `NowPlayingPlayback` authority - i.e. the iPhone
//! is the sole playback driver. iOS treats Consumer Control page (`0x0C`)
//! usages as system media keys and routes them to whichever app holds the
//! `MPNowPlayingInfoCenter` focus.
//!
//! Hardware events on the Car Thing (wheel rotation, presets, back/settings
//! buttons) are captured by the on-device webapp and never enter HID. HID
//! is one-way: accessory -> iPhone, button intents only.
//!
//! Three CSMs are sent by the accessory:
//!
//! - [`StartHID`] (`0x6800`) - declare a virtual HID device with a
//!   descriptor blob. Sent once per session after Identified.
//! - [`AccessoryHIDReport`] (`0x6802`) - one button-state report per event.
//!   Press = bit set; release = bit cleared. Always send a release after a
//!   press or iOS treats the button as held.
//! - [`StopHID`] (`0x6803`) - tear down the virtual HID device. Sent on
//!   session teardown.
//!
//! Cleanroom doc references: `protocol/30_control_session.md` (HID CSM
//! catalogue) and `bridgething/20_hid_descriptors.md` (descriptor
//! strategy).
//!
//! `0x6804` (DeviceHIDReport, iPhone -> accessory) exists for iOS-side
//! virtual HID devices but is not consumed by bridgething today.

use bytes::Bytes;

use super::Csm;

pub const SENT_BY_ACCESSORY: &[u16] = &[
  StartHID::CSM_MSG_ID,
  AccessoryHIDReport::CSM_MSG_ID,
  StopHID::CSM_MSG_ID,
];

pub const RECEIVED_BY_ACCESSORY: &[u16] = &[
  DeviceHIDReport::CSM_MSG_ID,
  StartNativeHID::CSM_MSG_ID,
  HIDComponentUpdate::CSM_MSG_ID,
];

/// Identifier the accessory uses to address its virtual HID device. The
/// value is opaque to iOS beyond uniqueness within a single iAP2 session;
/// it is intentionally reused as the BluetoothTransportComponent
/// identifier in `IdentificationInformation` so the two components share
/// one cid (this is how factory Car Things wire their identification up
/// and the shape iOS expects).
pub const TRANSPORT_COMPONENT_ID: u16 = 5353;

/// USB VID emitted on `StartHID` param 1. iOS rejects HID enablement
/// silently when these are absent - the param shape is not optional in
/// practice. `0x1D6B` is the Linux Foundation USB VID, openly published
/// for open-source projects to use without registration.
pub const VENDOR_ID: u16 = 0x1D6B;

/// USB PID emitted on `StartHID` param 2. Bridgething-unique value;
/// pairs with `VENDOR_ID` above. Not registered with anyone - iOS only
/// validates that param 2 is present and well-formed, not the value.
pub const PRODUCT_ID: u16 = 0xB31D;

/// USB-HID 1.11 descriptor for bridgething's outbound transport device.
/// Six Consumer Control usages packed into a single byte with two bits
/// of constant padding; no Report ID.
///
/// | Bit  | Usage               | Code |
/// | ---- | ------------------- | ---- |
/// | 0x01 | Play/Pause          | 0xCD |
/// | 0x02 | Scan Next Track     | 0xB5 |
/// | 0x04 | Scan Previous Track | 0xB6 |
/// | 0x08 | Volume Increment    | 0xE9 |
/// | 0x10 | Volume Decrement    | 0xEA |
/// | 0x20 | Mute                | 0xE2 |
/// | 0x40..0x80 | constant padding (no actuation) |
///
/// Wheel rotation, presets, and other physical inputs are NOT in this
/// descriptor; they are captured by the on-device webapp and never sent
/// to iOS.
pub const TRANSPORT_DESCRIPTOR: &[u8] = &[
  0x05, 0x0C, // Usage Page (Consumer)
  0x09, 0x01, // Usage (Consumer Control)
  0xA1, 0x01, // Collection (Application)
  0x15, 0x00, //   Logical Minimum (0)
  0x25, 0x01, //   Logical Maximum (1)
  0x75, 0x01, //   Report Size (1)
  0x95, 0x06, //   Report Count (6)
  0x09, 0xCD, //   Usage (Play/Pause)
  0x09, 0xB5, //   Usage (Scan Next Track)
  0x09, 0xB6, //   Usage (Scan Previous Track)
  0x09, 0xE9, //   Usage (Volume Increment)
  0x09, 0xEA, //   Usage (Volume Decrement)
  0x09, 0xE2, //   Usage (Mute)
  0x81, 0x02, //   Input (Data,Var,Abs)
  0x75, 0x02, //   Report Size (2)
  0x95, 0x01, //   Report Count (1)
  0x81, 0x03, //   Input (Const,Var,Abs) - padding
  0xC0, // End Collection
];

/// Bit positions inside the single-byte HID report payload. The whole byte
/// is the bitmap of currently-held buttons; multiple bits set in one report
/// is legal and means simultaneous holds.
pub mod report_bit {
  pub const PLAY_PAUSE: u8 = 0x01;
  pub const NEXT: u8 = 0x02;
  pub const PREV: u8 = 0x04;
  pub const VOLUME_UP: u8 = 0x08;
  pub const VOLUME_DOWN: u8 = 0x10;
  pub const MUTE: u8 = 0x20;
  /// Bits 0x40 / 0x80 fall inside the descriptor's 2-bit constant padding
  /// field. They will not actuate on iOS regardless of mask value; the
  /// constants exist so the transport controller's bit-mask plumbing
  /// stays compilable while shuffle / repeat live on other surfaces.
  pub const SHUFFLE: u8 = 0x40;
  pub const REPEAT: u8 = 0x80;
}

/// `0x6800` accessory -> iPhone. Declares a virtual HID device with the
/// given descriptor. iOS parses the descriptor and registers the device;
/// subsequent [`AccessoryHIDReport`]s on the same `component_id` are
/// dispatched to it.
///
/// Param layout matches what iOS validates in practice: cid at 0, USB
/// VID/PID at 1/2, and the descriptor at 4. iOS silently fails to enable
/// the HID component when VID/PID are missing or when the descriptor
/// lands at a different param id.
#[derive(Csm, Debug, Clone, PartialEq, Eq)]
#[csm(id = 0x6800)]
pub struct StartHID {
  #[csm(param = 0)]
  pub component_id: u16,
  #[csm(param = 1)]
  pub vendor_id: u16,
  #[csm(param = 2)]
  pub product_id: u16,
  #[csm(param = 4)]
  pub descriptor: Bytes,
}

/// `0x6802` accessory -> iPhone. One report per state change. The first
/// byte of `report` must be the Report ID byte declared in the descriptor;
/// the remaining bytes are the report payload (one byte for the transport
/// descriptor).
#[derive(Csm, Debug, Clone, PartialEq, Eq)]
#[csm(id = 0x6802)]
pub struct AccessoryHIDReport {
  #[csm(param = 0)]
  pub component_id: u16,
  #[csm(param = 1)]
  pub report: Bytes,
}

/// `0x6803` accessory -> iPhone. Tears down the virtual HID device matching
/// `component_id`. iOS will stop dispatching reports on that id; sending
/// further [`AccessoryHIDReport`]s on it after a stop is a protocol error.
#[derive(Csm, Debug, Clone, PartialEq, Eq)]
#[csm(id = 0x6803)]
pub struct StopHID {
  #[csm(param = 0)]
  pub component_id: u16,
}

/// `0x6801` iPhone -> accessory. Inbound HID report dispatched to a HID
/// component the accessory has declared. Same shape as outbound. Stock
/// Car Thing logs and drops these; bridgething follows suit.
#[derive(Csm, Debug, Clone, PartialEq, Eq)]
#[csm(id = 0x6801)]
pub struct DeviceHIDReport {
  #[csm(param = 0)]
  pub component_id: u16,
  #[csm(param = 1)]
  pub report: Bytes,
}

/// `0x6806` iPhone -> accessory. Signals iOS bringing the accessory's
/// native HID component online. No params.
#[derive(Csm, Debug, Clone, PartialEq, Eq)]
#[csm(id = 0x6806)]
pub struct StartNativeHID;

/// `0x6807` iPhone -> accessory. The "device is ready to consume reports
/// for this component" gate. Indicates whether the iPhone is willing to
/// route HID reports through the named component.
#[derive(Csm, Debug, Clone, PartialEq, Eq)]
#[csm(id = 0x6807)]
pub struct HIDComponentUpdate {
  #[csm(param = 0)]
  pub component_id: u16,
  #[csm(param = 1)]
  pub component_enabled: bool,
}

/// Build an [`AccessoryHIDReport`] for the bridgething transport
/// component. The single-byte payload is `[mask]` where `mask` is any
/// combination of [`report_bit`] flags (the all-zero mask is a release
/// frame, used to follow press frames). No leading Report ID byte -
/// the descriptor declares one report with no Report ID item.
pub fn transport_report(mask: u8) -> AccessoryHIDReport {
  AccessoryHIDReport {
    component_id: TRANSPORT_COMPONENT_ID,
    report: Bytes::copy_from_slice(&[mask]),
  }
}

#[cfg(test)]
mod tests {
  use super::{super::CsmFrame, *};

  #[test]
  fn start_hid_round_trips() {
    let original = StartHID {
      component_id: TRANSPORT_COMPONENT_ID,
      vendor_id: VENDOR_ID,
      product_id: PRODUCT_ID,
      descriptor: Bytes::copy_from_slice(TRANSPORT_DESCRIPTOR),
    };
    let frame: CsmFrame = original.clone().into();
    assert_eq!(frame.msg_id, 0x6800);
    let decoded: StartHID = frame.try_into().expect("decode");
    assert_eq!(decoded, original);
  }

  #[test]
  fn accessory_hid_report_round_trips() {
    let original = transport_report(report_bit::PLAY_PAUSE);
    let frame: CsmFrame = original.clone().into();
    assert_eq!(frame.msg_id, 0x6802);
    let decoded: AccessoryHIDReport = frame.try_into().expect("decode");
    assert_eq!(decoded, original);
    assert_eq!(decoded.report.as_ref(), &[0x01]);
  }

  #[test]
  fn stop_hid_round_trips() {
    let original = StopHID {
      component_id: TRANSPORT_COMPONENT_ID,
    };
    let frame: CsmFrame = original.clone().into();
    assert_eq!(frame.msg_id, 0x6803);
    let decoded: StopHID = frame.try_into().expect("decode");
    assert_eq!(decoded, original);
  }

  #[test]
  fn release_frame_is_zero_mask() {
    let release = transport_report(0);
    assert_eq!(release.report.as_ref(), &[0x00]);
  }

  #[test]
  fn descriptor_starts_with_consumer_page() {
    assert_eq!(&TRANSPORT_DESCRIPTOR[0..2], &[0x05, 0x0C]);
    assert_eq!(*TRANSPORT_DESCRIPTOR.last().unwrap(), 0xC0);
  }
}
