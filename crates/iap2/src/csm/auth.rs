use bytes::Bytes;

use super::Csm;

pub const SENT_BY_ACCESSORY: &[u16] = &[];
pub const RECEIVED_BY_ACCESSORY: &[u16] = &[];

#[derive(Csm, Debug, Clone, PartialEq, Eq)]
#[csm(id = 0xAA00)]
pub struct RequestAuthenticationCertificate;

#[derive(Csm, Debug, Clone, PartialEq, Eq)]
#[csm(id = 0xAA01)]
pub struct AuthenticationCertificate {
  #[csm(param = 0)]
  pub cert: Bytes,
}

#[derive(Csm, Debug, Clone, PartialEq, Eq)]
#[csm(id = 0xAA02)]
pub struct RequestAuthenticationChallengeResponse {
  #[csm(param = 0)]
  pub challenge: Bytes,
}

#[derive(Csm, Debug, Clone, PartialEq, Eq)]
#[csm(id = 0xAA03)]
pub struct AuthenticationResponse {
  #[csm(param = 0)]
  pub response: Bytes,
}

#[derive(Csm, Debug, Clone, PartialEq, Eq)]
#[csm(id = 0xAA04)]
pub struct AuthenticationFailed;

#[derive(Csm, Debug, Clone, PartialEq, Eq)]
#[csm(id = 0xAA05)]
pub struct AuthenticationSucceeded;

#[cfg(test)]
mod tests {
  use bytes::BytesMut;
  use tokio_util::codec::{Decoder, Encoder};

  use super::*;
  use crate::csm::{CsmCodec, CsmDecodeError, CsmFrame};

  #[test]
  fn empty_auth_csm_roundtrips_through_frame() {
    let cert_req = RequestAuthenticationCertificate;
    let frame: CsmFrame = cert_req.clone().into();
    assert_eq!(frame.msg_id, 0xAA00);
    assert!(frame.params.is_empty());
    let back: RequestAuthenticationCertificate = frame.try_into().unwrap();
    assert_eq!(back, cert_req);
  }

  #[test]
  fn cert_csm_carries_bytes_param_zero() {
    let cert = AuthenticationCertificate {
      cert: Bytes::from_static(&[0x30, 0x82, 0x01, 0x23]),
    };
    let frame: CsmFrame = cert.clone().into();
    assert_eq!(frame.msg_id, 0xAA01);
    assert_eq!(frame.params.len(), 1);
    assert_eq!(frame.params[0].id, 0);
    assert_eq!(&frame.params[0].payload[..], &[0x30, 0x82, 0x01, 0x23]);
    let back: AuthenticationCertificate = frame.try_into().unwrap();
    assert_eq!(back, cert);
  }

  #[test]
  fn challenge_response_roundtrips() {
    let chal = RequestAuthenticationChallengeResponse {
      challenge: Bytes::copy_from_slice(&[0xAA; 32]),
    };
    let frame: CsmFrame = chal.clone().into();
    assert_eq!(frame.msg_id, 0xAA02);

    let resp = AuthenticationResponse {
      response: Bytes::copy_from_slice(&[0xBB; 64]),
    };
    let frame: CsmFrame = resp.clone().into();
    assert_eq!(frame.msg_id, 0xAA03);
    assert_eq!(frame.params[0].payload.len(), 64);
    let back: AuthenticationResponse = frame.try_into().unwrap();
    assert_eq!(back, resp);
  }

  #[test]
  fn auth_failed_and_succeeded_have_no_params() {
    let f: CsmFrame = AuthenticationFailed.into();
    assert_eq!(f.msg_id, 0xAA04);
    assert!(f.params.is_empty());
    let s: CsmFrame = AuthenticationSucceeded.into();
    assert_eq!(s.msg_id, 0xAA05);
    assert!(s.params.is_empty());
  }

  #[test]
  fn try_from_rejects_wrong_msg_id() {
    let frame = CsmFrame::empty(0xAA00);
    let err = AuthenticationSucceeded::try_from(frame).unwrap_err();
    assert!(matches!(
      err,
      CsmDecodeError::WrongMsgId {
        got: 0xAA00,
        expected: 0xAA05,
      }
    ));
  }

  #[test]
  fn auth_csm_round_trip_through_codec() {
    let cert = AuthenticationCertificate {
      cert: Bytes::copy_from_slice(&[0x30, 0x82, 0x05, 0x40, 0x42, 0x21, 0x99]),
    };
    let mut buf = BytesMut::new();
    let frame: CsmFrame = cert.clone().into();
    CsmCodec.encode(frame, &mut buf).unwrap();
    let decoded = CsmCodec.decode(&mut buf).unwrap().unwrap();
    let back: AuthenticationCertificate = decoded.try_into().unwrap();
    assert_eq!(back, cert);
  }

  #[test]
  fn supported_messages_lists_match_msg_ids() {
    assert!(SENT_BY_ACCESSORY.is_empty());
    assert!(RECEIVED_BY_ACCESSORY.is_empty());
  }
}
