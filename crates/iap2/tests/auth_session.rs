//! End-to-end tests for the iAP2 control session: drives a fake peer
//! that completes the link handshake, walks the auth + identification
//! exchange, and asserts the [`Iap2Session`] task emits
//! `Authenticated` then `Identified`. Two more tests cover the
//! `AuthenticationFailed` and `IdentificationRejected` paths. All
//! three use a hand-rolled [`FakeMfi`] impl of [`MfiAccess`] so the
//! tests do not need to import or run a `MockTransport`.

mod common;

use std::time::Duration;

use async_trait::async_trait;
use bridgething_iap2::csm::auth::{
  AuthenticationCertificate, AuthenticationFailed, AuthenticationResponse, AuthenticationSucceeded,
  RequestAuthenticationCertificate, RequestAuthenticationChallengeResponse,
};
use bridgething_iap2::csm::identification::{
  CarthingIdentification, IdentificationAccepted, IdentificationConfig, IdentificationInformation,
  IdentificationRejected, StartIdentification,
};
use bridgething_iap2::csm::{CsmCodec, CsmFrame};
use bridgething_iap2::{ControlBits, Iap2Session, LinkCodec, LinkPacket, MfiAccess, SessionEvent};
use bridgething_mfi::{CHALLENGE_LEN, Error as MfiError, RESPONSE_LEN};
use bytes::{Bytes, BytesMut};
use tokio::io::DuplexStream;
use tokio::sync::mpsc;
use tokio_util::codec::{Decoder, Encoder};

use common::{
  LspBuilder, PEER_INITIAL_PSN, drive_peer_handshake, fast_link_config, read_link, recv_with_timeout, spawn_link,
  write_link,
};

const CONTROL_SESSION_ID: u8 = 0;

fn accessory_lsp() -> bridgething_iap2::Lsp {
  LspBuilder::default().build()
}

fn peer_lsp() -> bridgething_iap2::Lsp {
  LspBuilder {
    max_outgoing: 127,
    max_len: 65535,
    ..LspBuilder::default()
  }
  .build()
}

fn identification_config() -> IdentificationConfig {
  IdentificationConfig::for_carthing(CarthingIdentification {
    serial_number: "BT-TEST-0001".into(),
    firmware_version: "v0.1.0".into(),
    bt_mac: [0x11, 0x22, 0x33, 0x44, 0x55, 0x66],
  })
}

#[derive(Clone)]
struct FakeMfi {
  cert_bytes: Bytes,
  signature: [u8; RESPONSE_LEN],
}

impl FakeMfi {
  fn ok() -> Self {
    Self {
      cert_bytes: Bytes::from_static(b"FAKE-MFI-CERT-DER"),
      signature: [0xAB; RESPONSE_LEN],
    }
  }
}

#[async_trait]
impl MfiAccess for FakeMfi {
  async fn cert(&mut self) -> Result<Bytes, MfiError> {
    Ok(self.cert_bytes.clone())
  }

  async fn sign(&mut self, _challenge: [u8; CHALLENGE_LEN]) -> Result<[u8; RESPONSE_LEN], MfiError> {
    Ok(self.signature)
  }
}

struct Harness {
  peer: DuplexStream,
  peer_buf: BytesMut,
  peer_codec: LinkCodec,
  control_buf: BytesMut,
  session_events_rx: mpsc::Receiver<SessionEvent>,
  our_initial_psn: u8,
  peer_seq: u8,
}

impl Harness {
  async fn establish(mfi: FakeMfi) -> Self {
    let (mut peer, link_command_tx, link_events_rx, _link) = spawn_link(fast_link_config(accessory_lsp()));
    let (session_events_tx, mut session_events_rx) = mpsc::channel::<SessionEvent>(32);

    let session = Iap2Session::new(
      identification_config(),
      mfi,
      link_command_tx,
      link_events_rx,
      session_events_tx,
    );
    tokio::spawn(session.run());

    let (peer_buf, peer_codec, our_initial_psn) = drive_peer_handshake(&mut peer, peer_lsp()).await;

    let evt = recv_with_timeout(&mut session_events_rx, Duration::from_secs(2))
      .await
      .expect("LinkEstablished");
    assert!(matches!(evt, SessionEvent::LinkEstablished(_)));

    Self {
      peer,
      peer_buf,
      peer_codec,
      control_buf: BytesMut::new(),
      session_events_rx,
      our_initial_psn,
      peer_seq: PEER_INITIAL_PSN,
    }
  }

  fn our_ack(&self) -> u8 {
    self.our_initial_psn.wrapping_add(1)
  }

  async fn send_csm<F: Into<CsmFrame>>(&mut self, csm: F) {
    self.peer_seq = self.peer_seq.wrapping_add(1);
    let mut buf = BytesMut::new();
    CsmCodec.encode(csm.into(), &mut buf).unwrap();
    let pkt = LinkPacket::with_payload(
      ControlBits::ACK,
      self.peer_seq,
      self.our_ack(),
      CONTROL_SESSION_ID,
      buf.freeze(),
    );
    write_link(&mut self.peer, &mut self.peer_codec, pkt).await;
  }

  async fn read_csm(&mut self) -> CsmFrame {
    loop {
      if let Some(frame) = CsmCodec.decode(&mut self.control_buf).expect("csm decode") {
        return frame;
      }
      let pkt = read_link(&mut self.peer, &mut self.peer_buf, &mut self.peer_codec).await;
      if pkt.header.has_payload() && !pkt.header.control.contains(ControlBits::SYN) {
        self.control_buf.extend_from_slice(&pkt.payload);
      }
    }
  }

  async fn next_event(&mut self) -> SessionEvent {
    recv_with_timeout(&mut self.session_events_rx, Duration::from_secs(3))
      .await
      .expect("session event before timeout")
  }
}

#[tokio::test(flavor = "current_thread")]
async fn auth_succeeds_then_identification_completes() {
  let mut h = Harness::establish(FakeMfi::ok()).await;

  h.send_csm(RequestAuthenticationCertificate).await;
  let cert: AuthenticationCertificate = h.read_csm().await.try_into().unwrap();
  assert_eq!(&cert.cert[..], FakeMfi::ok().cert_bytes.as_ref());

  h.send_csm(RequestAuthenticationChallengeResponse {
    challenge: Bytes::from_static(&[0x11; CHALLENGE_LEN]),
  })
  .await;
  let resp: AuthenticationResponse = h.read_csm().await.try_into().unwrap();
  assert_eq!(resp.response.len(), RESPONSE_LEN);
  assert!(resp.response.iter().all(|b| *b == 0xAB));

  h.send_csm(AuthenticationSucceeded).await;
  assert!(matches!(h.next_event().await, SessionEvent::Authenticated));

  h.send_csm(StartIdentification).await;
  let info = h.read_csm().await;
  assert_eq!(info.msg_id, IdentificationInformation::CSM_MSG_ID);
  assert!(info.find(0).is_some(), "name param");
  assert!(info.find(2).is_some(), "manufacturer param");
  assert!(info.find(17).is_some(), "BT transport param");

  h.send_csm(IdentificationAccepted).await;
  assert!(matches!(h.next_event().await, SessionEvent::Identified));
}

#[tokio::test(flavor = "current_thread")]
async fn auth_failed_drives_session_to_disconnect() {
  let mut h = Harness::establish(FakeMfi::ok()).await;

  h.send_csm(AuthenticationFailed).await;
  assert!(matches!(h.next_event().await, SessionEvent::AuthFailed));
  assert!(matches!(h.next_event().await, SessionEvent::LinkDown(_)));
}

#[tokio::test(flavor = "current_thread")]
async fn identification_rejected_propagates_failed_param_ids() {
  let mut h = Harness::establish(FakeMfi::ok()).await;

  h.send_csm(RequestAuthenticationCertificate).await;
  let _ = h.read_csm().await;
  h.send_csm(RequestAuthenticationChallengeResponse {
    challenge: Bytes::from_static(&[0x22; CHALLENGE_LEN]),
  })
  .await;
  let _ = h.read_csm().await;
  h.send_csm(AuthenticationSucceeded).await;
  assert!(matches!(h.next_event().await, SessionEvent::Authenticated));

  h.send_csm(StartIdentification).await;
  let _ = h.read_csm().await;

  h.send_csm(IdentificationRejected {
    rejected_params: vec![3, 17],
  })
  .await;

  match h.next_event().await {
    SessionEvent::IdentificationRejected { rejected_params } => assert_eq!(rejected_params, vec![3, 17]),
    other => panic!("expected IdentificationRejected, got {:?}", other),
  }
  assert!(matches!(h.next_event().await, SessionEvent::LinkDown(_)));
}
