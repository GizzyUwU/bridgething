//! Integration tests for the iAP2 link's Established phase: DATA send +
//! recv, ACK piggyback, retransmit, EAK, window backpressure, ack-delay.
//! Drives a hand-rolled fake peer over `tokio::io::duplex` with short
//! timing values so retransmit + ack-delay fire within a few hundred ms.

use std::time::Duration;

use bridgething_iap2::{
  ControlBits, DETECT_MARKER, Error, Iap2Command, Iap2Event, LINK_HEADER_LEN, Link, LinkCodec, LinkConfig, LinkPacket,
  Lsp, SessionTriple,
};
use bytes::{Bytes, BytesMut};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, DuplexStream};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_util::codec::{Decoder, Encoder};

const OUR_INITIAL_PSN: u8 = 99;
const PEER_INITIAL_PSN: u8 = 50;
const SESSION_ID: u8 = 1;

fn our_lsp() -> Lsp {
  Lsp {
    version: 1,
    max_outgoing: 5,
    max_len: 2048,
    retransmission_timeout_ms: 6000,
    ack_timeout_ms: 3000,
    max_retransmissions: 30,
    max_ack: 3,
    sessions: vec![SessionTriple {
      id: SESSION_ID,
      session_type: 0,
      version: 1,
    }],
  }
}

#[derive(Debug, Clone)]
struct PeerProposal {
  max_outgoing: u8,
  max_len: u16,
  retransmission_timeout_ms: u16,
  ack_timeout_ms: u16,
  max_retransmissions: u8,
  max_ack: u8,
}

impl Default for PeerProposal {
  fn default() -> Self {
    Self {
      max_outgoing: 5,
      max_len: 2048,
      retransmission_timeout_ms: 6000,
      ack_timeout_ms: 3000,
      max_retransmissions: 30,
      max_ack: 3,
    }
  }
}

impl PeerProposal {
  fn into_lsp(self) -> Lsp {
    Lsp {
      version: 1,
      max_outgoing: self.max_outgoing,
      max_len: self.max_len,
      retransmission_timeout_ms: self.retransmission_timeout_ms,
      ack_timeout_ms: self.ack_timeout_ms,
      max_retransmissions: self.max_retransmissions,
      max_ack: self.max_ack,
      sessions: vec![SessionTriple {
        id: SESSION_ID,
        session_type: 0,
        version: 1,
      }],
    }
  }
}

struct Established {
  events_rx: mpsc::Receiver<Iap2Event>,
  cmd_tx: mpsc::Sender<Iap2Command>,
  peer: DuplexStream,
  peer_buf: BytesMut,
  peer_codec: LinkCodec,
  link: JoinHandle<Result<(), Error>>,
}

async fn establish(peer: PeerProposal) -> Established {
  let (us, mut peer_stream) = tokio::io::duplex(8192);
  let (events_tx, mut events_rx) = mpsc::channel(32);
  let (cmd_tx, cmd_rx) = mpsc::channel::<Iap2Command>(32);

  let mut config = LinkConfig::new(our_lsp());
  config.detect_interval = Duration::from_millis(50);
  config.handshake_timeout = Duration::from_secs(5);
  let link = tokio::spawn(Link::run(us, config, events_tx, cmd_rx));

  // Peer side: send detect, send SYN, read our SYN, read our ACK.
  peer_stream.write_all(&DETECT_MARKER).await.unwrap();
  let mut peer_codec = LinkCodec;
  let peer_lsp = peer.into_lsp();
  let syn = LinkPacket::with_payload(ControlBits::SYN, PEER_INITIAL_PSN, 0, 0, peer_lsp.encode());
  write_packet_to(&mut peer_stream, &mut peer_codec, syn).await;

  let mut peer_buf = BytesMut::with_capacity(256);
  let _our_syn = read_packet_from(&mut peer_stream, &mut peer_buf, &mut peer_codec).await;
  let _our_ack = read_packet_from(&mut peer_stream, &mut peer_buf, &mut peer_codec).await;

  let event = tokio::time::timeout(Duration::from_secs(2), events_rx.recv())
    .await
    .unwrap()
    .unwrap();
  assert!(matches!(event, Iap2Event::Established(_)));

  Established {
    events_rx,
    cmd_tx,
    peer: peer_stream,
    peer_buf,
    peer_codec,
    link,
  }
}

async fn write_packet_to<W: AsyncWrite + Unpin>(writer: &mut W, codec: &mut LinkCodec, packet: LinkPacket) {
  let mut wire = BytesMut::new();
  codec.encode(packet, &mut wire).expect("encode");
  writer.write_all(&wire).await.expect("write_all");
  writer.flush().await.expect("flush");
}

async fn read_packet_from<R: AsyncRead + Unpin>(
  reader: &mut R,
  buf: &mut BytesMut,
  codec: &mut LinkCodec,
) -> LinkPacket {
  loop {
    if let Some(pkt) = codec.decode(buf).expect("codec decode") {
      return pkt;
    }
    let n = reader.read_buf(buf).await.expect("read_buf");
    assert!(n > 0, "stream closed before a packet decoded");
  }
}

#[tokio::test(flavor = "current_thread")]
async fn send_command_data_round_trips_to_peer() {
  let mut e = establish(PeerProposal::default()).await;

  e.cmd_tx
    .send(Iap2Command::Send {
      session_id: SESSION_ID,
      payload: Bytes::from_static(b"hello"),
    })
    .await
    .unwrap();

  let pkt = read_packet_from(&mut e.peer, &mut e.peer_buf, &mut e.peer_codec).await;
  assert!(pkt.header.control.contains(ControlBits::ACK));
  assert!(!pkt.header.control.contains(ControlBits::SYN));
  assert_eq!(pkt.header.seq, OUR_INITIAL_PSN.wrapping_add(1));
  assert_eq!(pkt.header.ack, PEER_INITIAL_PSN.wrapping_add(1));
  assert_eq!(pkt.header.session_id, SESSION_ID);
  assert_eq!(pkt.payload.as_ref(), b"hello");
}

#[tokio::test(flavor = "current_thread")]
async fn large_payload_fragments_into_chunks() {
  // max_len 60 → max payload per packet = 50 (60 - 9 header - 1 csum).
  let mut e = establish(PeerProposal {
    max_len: 60,
    ..PeerProposal::default()
  })
  .await;

  let total = Bytes::from(vec![0xAB; 50 + 50 + 5]);
  e.cmd_tx
    .send(Iap2Command::Send {
      session_id: SESSION_ID,
      payload: total,
    })
    .await
    .unwrap();

  let p1 = read_packet_from(&mut e.peer, &mut e.peer_buf, &mut e.peer_codec).await;
  let p2 = read_packet_from(&mut e.peer, &mut e.peer_buf, &mut e.peer_codec).await;
  let p3 = read_packet_from(&mut e.peer, &mut e.peer_buf, &mut e.peer_codec).await;

  assert_eq!(p1.payload.len(), 50);
  assert_eq!(p2.payload.len(), 50);
  assert_eq!(p3.payload.len(), 5);
  assert_eq!(p1.header.seq, OUR_INITIAL_PSN.wrapping_add(1));
  assert_eq!(p2.header.seq, OUR_INITIAL_PSN.wrapping_add(2));
  assert_eq!(p3.header.seq, OUR_INITIAL_PSN.wrapping_add(3));
}

#[tokio::test(flavor = "current_thread")]
async fn inbound_data_delivers_to_events_channel() {
  let mut e = establish(PeerProposal::default()).await;

  let pkt = LinkPacket::with_payload(
    ControlBits::ACK,
    PEER_INITIAL_PSN.wrapping_add(1),
    OUR_INITIAL_PSN.wrapping_add(1),
    SESSION_ID,
    Bytes::from_static(b"ping"),
  );
  write_packet_to(&mut e.peer, &mut e.peer_codec, pkt).await;

  let event = tokio::time::timeout(Duration::from_secs(2), e.events_rx.recv())
    .await
    .unwrap()
    .unwrap();
  match event {
    Iap2Event::DataReceived { session_id, payload } => {
      assert_eq!(session_id, SESSION_ID);
      assert_eq!(payload.as_ref(), b"ping");
    }
    other => panic!("expected DataReceived, got {:?}", other),
  }
}

#[tokio::test(flavor = "current_thread")]
async fn window_backpressures_on_unacked_max_outgoing() {
  let mut e = establish(PeerProposal {
    max_outgoing: 2,
    ..PeerProposal::default()
  })
  .await;

  for c in [b"a", b"b", b"c"] {
    e.cmd_tx
      .send(Iap2Command::Send {
        session_id: SESSION_ID,
        payload: Bytes::copy_from_slice(c),
      })
      .await
      .unwrap();
  }

  let p1 = read_packet_from(&mut e.peer, &mut e.peer_buf, &mut e.peer_codec).await;
  let p2 = read_packet_from(&mut e.peer, &mut e.peer_buf, &mut e.peer_codec).await;
  assert_eq!(p1.payload.as_ref(), b"a");
  assert_eq!(p2.payload.as_ref(), b"b");

  // Third packet should NOT arrive until we ACK at least one.
  let timeout = tokio::time::timeout(
    Duration::from_millis(75),
    read_packet_from(&mut e.peer, &mut e.peer_buf, &mut e.peer_codec),
  )
  .await;
  assert!(timeout.is_err(), "third packet leaked through closed window");

  // ACK the first; window opens by one slot, third packet flows.
  let ack = LinkPacket::header_only(ControlBits::ACK, PEER_INITIAL_PSN, OUR_INITIAL_PSN.wrapping_add(2));
  write_packet_to(&mut e.peer, &mut e.peer_codec, ack).await;

  let p3 = read_packet_from(&mut e.peer, &mut e.peer_buf, &mut e.peer_codec).await;
  assert_eq!(p3.payload.as_ref(), b"c");
  assert_eq!(p3.header.seq, OUR_INITIAL_PSN.wrapping_add(3));
}

#[tokio::test(flavor = "current_thread")]
async fn retransmit_resends_unacked_packet_after_timeout() {
  let mut e = establish(PeerProposal {
    retransmission_timeout_ms: 100,
    max_retransmissions: 5,
    ..PeerProposal::default()
  })
  .await;

  e.cmd_tx
    .send(Iap2Command::Send {
      session_id: SESSION_ID,
      payload: Bytes::from_static(b"ouch"),
    })
    .await
    .unwrap();

  let first = read_packet_from(&mut e.peer, &mut e.peer_buf, &mut e.peer_codec).await;
  let resend = read_packet_from(&mut e.peer, &mut e.peer_buf, &mut e.peer_codec).await;

  assert_eq!(first.header.seq, resend.header.seq);
  assert_eq!(first.payload, resend.payload);
}

#[tokio::test(flavor = "current_thread")]
async fn max_retransmissions_drives_link_down() {
  let mut e = establish(PeerProposal {
    retransmission_timeout_ms: 30,
    max_retransmissions: 2,
    ..PeerProposal::default()
  })
  .await;

  e.cmd_tx
    .send(Iap2Command::Send {
      session_id: SESSION_ID,
      payload: Bytes::from_static(b"doomed"),
    })
    .await
    .unwrap();

  // Peer reads but never ACKs. Two retransmit attempts then we declare dead.
  let _ = read_packet_from(&mut e.peer, &mut e.peer_buf, &mut e.peer_codec).await;
  let _ = read_packet_from(&mut e.peer, &mut e.peer_buf, &mut e.peer_codec).await;
  let _ = read_packet_from(&mut e.peer, &mut e.peer_buf, &mut e.peer_codec).await;

  let event = tokio::time::timeout(Duration::from_secs(2), e.events_rx.recv())
    .await
    .unwrap()
    .unwrap();
  match event {
    Iap2Event::LinkDown(reason) => assert!(reason.contains("retransmit"), "got reason {:?}", reason),
    other => panic!("expected LinkDown, got {:?}", other),
  }

  let result = tokio::time::timeout(Duration::from_secs(2), e.link)
    .await
    .unwrap()
    .unwrap();
  assert!(matches!(result, Err(Error::RetransmitLimit)));
}

#[tokio::test(flavor = "current_thread")]
async fn ack_delay_fires_standalone_ack_when_no_outbound_to_piggyback() {
  let mut e = establish(PeerProposal {
    ack_timeout_ms: 100,
    max_ack: 100,
    ..PeerProposal::default()
  })
  .await;

  let inbound = LinkPacket::with_payload(
    ControlBits::ACK,
    PEER_INITIAL_PSN.wrapping_add(1),
    OUR_INITIAL_PSN.wrapping_add(1),
    SESSION_ID,
    Bytes::from_static(b"ping"),
  );
  write_packet_to(&mut e.peer, &mut e.peer_codec, inbound).await;

  // Drain DataReceived event to let the loop progress.
  let _ = tokio::time::timeout(Duration::from_secs(2), e.events_rx.recv())
    .await
    .unwrap()
    .unwrap();

  let ack = read_packet_from(&mut e.peer, &mut e.peer_buf, &mut e.peer_codec).await;
  assert!(ack.header.control.contains(ControlBits::ACK));
  assert!(!ack.header.has_payload());
  assert_eq!(ack.header.length as usize, LINK_HEADER_LEN);
  assert_eq!(ack.header.ack, PEER_INITIAL_PSN.wrapping_add(2));
}

#[tokio::test(flavor = "current_thread")]
async fn cumulative_max_ack_threshold_fires_standalone_ack() {
  let mut e = establish(PeerProposal {
    ack_timeout_ms: 5000,
    max_ack: 2,
    ..PeerProposal::default()
  })
  .await;

  for i in 1..=2u8 {
    let pkt = LinkPacket::with_payload(
      ControlBits::ACK,
      PEER_INITIAL_PSN.wrapping_add(i),
      OUR_INITIAL_PSN.wrapping_add(1),
      SESSION_ID,
      Bytes::from_static(b"x"),
    );
    write_packet_to(&mut e.peer, &mut e.peer_codec, pkt).await;
  }

  for _ in 0..2 {
    let _ = tokio::time::timeout(Duration::from_secs(2), e.events_rx.recv())
      .await
      .unwrap()
      .unwrap();
  }

  let ack = read_packet_from(&mut e.peer, &mut e.peer_buf, &mut e.peer_codec).await;
  assert!(ack.header.control.contains(ControlBits::ACK));
  assert!(!ack.header.has_payload());
  assert_eq!(ack.header.ack, PEER_INITIAL_PSN.wrapping_add(3));
}

#[tokio::test(flavor = "current_thread")]
async fn out_of_order_inbound_triggers_eak_listing_missing_psns() {
  let mut e = establish(PeerProposal::default()).await;

  // Peer sends seq=peer+2, skipping peer+1.
  let gap = LinkPacket::with_payload(
    ControlBits::ACK,
    PEER_INITIAL_PSN.wrapping_add(2),
    OUR_INITIAL_PSN.wrapping_add(1),
    SESSION_ID,
    Bytes::from_static(b"future"),
  );
  write_packet_to(&mut e.peer, &mut e.peer_codec, gap).await;

  let eak = read_packet_from(&mut e.peer, &mut e.peer_buf, &mut e.peer_codec).await;
  assert!(eak.header.control.contains(ControlBits::EAK));
  assert_eq!(eak.payload.as_ref(), &[PEER_INITIAL_PSN.wrapping_add(1)]);
}

#[tokio::test(flavor = "current_thread")]
async fn out_of_order_drains_in_order_when_gap_arrives() {
  let mut e = establish(PeerProposal::default()).await;

  // Send peer+2 first (out of order), then peer+1 (fills the gap).
  let p2 = LinkPacket::with_payload(
    ControlBits::ACK,
    PEER_INITIAL_PSN.wrapping_add(2),
    OUR_INITIAL_PSN.wrapping_add(1),
    SESSION_ID,
    Bytes::from_static(b"two"),
  );
  write_packet_to(&mut e.peer, &mut e.peer_codec, p2).await;

  // Drain the EAK so we don't muddle later assertions.
  let eak = read_packet_from(&mut e.peer, &mut e.peer_buf, &mut e.peer_codec).await;
  assert!(eak.header.control.contains(ControlBits::EAK));

  let p1 = LinkPacket::with_payload(
    ControlBits::ACK,
    PEER_INITIAL_PSN.wrapping_add(1),
    OUR_INITIAL_PSN.wrapping_add(1),
    SESSION_ID,
    Bytes::from_static(b"one"),
  );
  write_packet_to(&mut e.peer, &mut e.peer_codec, p1).await;

  let first = tokio::time::timeout(Duration::from_secs(2), e.events_rx.recv())
    .await
    .unwrap()
    .unwrap();
  match first {
    Iap2Event::DataReceived { payload, .. } => assert_eq!(payload.as_ref(), b"one"),
    other => panic!("expected DataReceived 'one', got {:?}", other),
  }
  let second = tokio::time::timeout(Duration::from_secs(2), e.events_rx.recv())
    .await
    .unwrap()
    .unwrap();
  match second {
    Iap2Event::DataReceived { payload, .. } => assert_eq!(payload.as_ref(), b"two"),
    other => panic!("expected DataReceived 'two', got {:?}", other),
  }
}
