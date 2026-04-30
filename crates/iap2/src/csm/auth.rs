//! Typed CSMs for the iAP2 authentication handshake.
//!
//! Five messages exchanged on the control session, gated before
//! `IdentificationAccepted` will arrive. iPhone drives certificate
//! retrieval and challenge issuance; the accessory replies via the
//! MFi coprocessor. See cleanroom doc `protocol/50_authentication.md`.
//!
//! The auth CSMs are flat structs with at most a single `Bytes` field;
//! the `Csm` derive generates `From<X> for CsmFrame` and
//! `TryFrom<CsmFrame> for X` for each.

use bytes::Bytes;

use super::Csm;

/// CSMs the accessory sends. Used to populate
/// `IdentificationInformation::messages_sent_by_accessory`.
pub const SENT_BY_ACCESSORY: &[u16] = &[
  AuthenticationCertificate::CSM_MSG_ID,
  AuthenticationResponse::CSM_MSG_ID,
];

/// CSMs the accessory accepts. Used to populate
/// `IdentificationInformation::messages_received_from_accessory`.
pub const RECEIVED_BY_ACCESSORY: &[u16] = &[
  RequestAuthenticationCertificate::CSM_MSG_ID,
  RequestAuthenticationChallengeResponse::CSM_MSG_ID,
  AuthenticationFailed::CSM_MSG_ID,
  AuthenticationSucceeded::CSM_MSG_ID,
];

/// `0xAA00` iPhone -> accessory. Asks the accessory to read its MFi
/// certificate from the coprocessor.
#[derive(Csm, Debug, Clone, PartialEq, Eq)]
#[csm(id = 0xAA00)]
pub struct RequestAuthenticationCertificate;

/// `0xAA01` accessory -> iPhone. Carries the X.509 DER certificate
/// read from the chip.
#[derive(Csm, Debug, Clone, PartialEq, Eq)]
#[csm(id = 0xAA01)]
pub struct AuthenticationCertificate {
  #[csm(param = 0)]
  pub cert: Bytes,
}

/// `0xAA02` iPhone -> accessory. Carries the random challenge bytes
/// (32 on CP3.0 chips) the accessory must sign.
#[derive(Csm, Debug, Clone, PartialEq, Eq)]
#[csm(id = 0xAA02)]
pub struct RequestAuthenticationChallengeResponse {
  #[csm(param = 0)]
  pub challenge: Bytes,
}

/// `0xAA03` accessory -> iPhone. Carries the signed response from the
/// coprocessor (64 bytes for CP3.0 ECDSA).
#[derive(Csm, Debug, Clone, PartialEq, Eq)]
#[csm(id = 0xAA03)]
pub struct AuthenticationResponse {
  #[csm(param = 0)]
  pub response: Bytes,
}

/// `0xAA04` iPhone -> accessory. The accessory should tear down the
/// link with a RST and not retry on the same RFCOMM connection.
#[derive(Csm, Debug, Clone, PartialEq, Eq)]
#[csm(id = 0xAA04)]
pub struct AuthenticationFailed;

/// `0xAA05` iPhone -> accessory. Authentication completed; the
/// accessory may proceed to identification.
#[derive(Csm, Debug, Clone, PartialEq, Eq)]
#[csm(id = 0xAA05)]
pub struct AuthenticationSucceeded;

#[cfg(test)]
mod tests {
  use super::*;
  use crate::csm::{CsmCodec, CsmDecodeError, CsmFrame};
  use bytes::BytesMut;
  use tokio_util::codec::{Decoder, Encoder};

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
    assert_eq!(SENT_BY_ACCESSORY, &[0xAA01, 0xAA03]);
    assert_eq!(RECEIVED_BY_ACCESSORY, &[0xAA00, 0xAA02, 0xAA04, 0xAA05]);
  }
}
