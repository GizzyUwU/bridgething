use super::Csm;

pub const SENT_BY_ACCESSORY: &[u16] = &[];
pub const RECEIVED_BY_ACCESSORY: &[u16] = &[
  DeviceInformationUpdate::CSM_MSG_ID,
  DeviceLanguageUpdate::CSM_MSG_ID,
  DeviceTimeUpdate::CSM_MSG_ID,
  DeviceUUIDUpdate::CSM_MSG_ID,
];

#[derive(Csm, Debug, Clone, PartialEq, Eq)]
#[csm(id = 0x4E09)]
pub struct DeviceInformationUpdate {
  #[csm(param = 0)]
  pub device_name: String,
}

#[derive(Csm, Debug, Clone, PartialEq, Eq)]
#[csm(id = 0x4E0A)]
pub struct DeviceLanguageUpdate {
  #[csm(param = 0)]
  pub language: String,
}

#[derive(Csm, Debug, Clone, PartialEq, Eq)]
#[csm(id = 0x4E0B)]
pub struct DeviceTimeUpdate {
  #[csm(param = 0)]
  pub seconds_since_reference_date: i64,
  #[csm(param = 1)]
  pub tz_offset_minutes: i16,
  #[csm(param = 2)]
  pub dst_offset_minutes: i8,
}

#[derive(Csm, Debug, Clone, PartialEq, Eq)]
#[csm(id = 0x4E0C)]
pub struct DeviceUUIDUpdate {
  #[csm(param = 0)]
  pub uuid: String,
}

#[cfg(test)]
mod tests {
  use super::{super::CsmFrame, *};

  #[test]
  fn device_name_round_trips() {
    let original = DeviceInformationUpdate {
      device_name: "Joey's iPhone".into(),
    };
    let frame: CsmFrame = original.clone().into();
    assert_eq!(frame.msg_id, 0x4E09);
    let decoded: DeviceInformationUpdate = frame.try_into().unwrap();
    assert_eq!(decoded, original);
  }

  #[test]
  fn device_language_round_trips() {
    let original = DeviceLanguageUpdate { language: "en".into() };
    let frame: CsmFrame = original.clone().into();
    assert_eq!(frame.msg_id, 0x4E0A);
    let decoded: DeviceLanguageUpdate = frame.try_into().unwrap();
    assert_eq!(decoded, original);
  }

  #[test]
  fn device_time_round_trips() {
    let original = DeviceTimeUpdate {
      seconds_since_reference_date: 1_777_777_777,
      tz_offset_minutes: -360,
      dst_offset_minutes: 60,
    };
    let frame: CsmFrame = original.clone().into();
    assert_eq!(frame.msg_id, 0x4E0B);
    let decoded: DeviceTimeUpdate = frame.try_into().unwrap();
    assert_eq!(decoded, original);
  }

  #[test]
  fn device_uuid_round_trips() {
    let original = DeviceUUIDUpdate {
      uuid: "550e8400-e29b-41d4-a716-446655440000".into(),
    };
    let frame: CsmFrame = original.clone().into();
    assert_eq!(frame.msg_id, 0x4E0C);
    let decoded: DeviceUUIDUpdate = frame.try_into().unwrap();
    assert_eq!(decoded, original);
  }
}
