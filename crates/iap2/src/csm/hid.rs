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

pub const TRANSPORT_COMPONENT_ID: u16 = 5353;
pub const VENDOR_ID: u16 = 0x1D6B;
pub const PRODUCT_ID: u16 = 0xB31D;

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

pub mod report_bit {
  pub const PLAY_PAUSE: u8 = 0x01;
  pub const NEXT: u8 = 0x02;
  pub const PREV: u8 = 0x04;
  pub const VOLUME_UP: u8 = 0x08;
  pub const VOLUME_DOWN: u8 = 0x10;
  pub const MUTE: u8 = 0x20;
  pub const SHUFFLE: u8 = 0x40;
  pub const REPEAT: u8 = 0x80;
}

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

#[derive(Csm, Debug, Clone, PartialEq, Eq)]
#[csm(id = 0x6802)]
pub struct AccessoryHIDReport {
  #[csm(param = 0)]
  pub component_id: u16,
  #[csm(param = 1)]
  pub report: Bytes,
}

#[derive(Csm, Debug, Clone, PartialEq, Eq)]
#[csm(id = 0x6803)]
pub struct StopHID {
  #[csm(param = 0)]
  pub component_id: u16,
}

#[derive(Csm, Debug, Clone, PartialEq, Eq)]
#[csm(id = 0x6801)]
pub struct DeviceHIDReport {
  #[csm(param = 0)]
  pub component_id: u16,
  #[csm(param = 1)]
  pub report: Bytes,
}

#[derive(Csm, Debug, Clone, PartialEq, Eq)]
#[csm(id = 0x6806)]
pub struct StartNativeHID;

#[derive(Csm, Debug, Clone, PartialEq, Eq)]
#[csm(id = 0x6807)]
pub struct HIDComponentUpdate {
  #[csm(param = 0)]
  pub component_id: u16,
  #[csm(param = 1)]
  pub component_enabled: bool,
}

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
