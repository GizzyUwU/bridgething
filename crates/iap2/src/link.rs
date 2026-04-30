//! iAP2 link-layer state machine: drives the byte stream from initial
//! detect handshake through SYN negotiation to Established.
//!
//! Scope of this wedge: detect → SYN → standalone-ACK → Established →
//! idle until peer disconnects, peer sends RST, or daemon issues
//! `Iap2Command::Disconnect`. Retransmit timer, ack-delay timer, EAK,
//! DATA dispatch, and CSM-level handlers land in subsequent slices.

use std::time::Duration;

use bytes::BytesMut;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::mpsc;
use tokio_util::codec::{Decoder, Encoder};

use crate::error::{Error, Result};
use crate::frame::{ControlBits, DETECT_MARKER, LINK_MAGIC, LinkCodec, LinkPacket, Lsp};

const READ_CAPACITY: usize = 4096;

#[derive(Debug, Clone)]
pub struct LinkConfig {
  /// First sequence number we'll stamp on outbound packets.
  pub initial_psn: u8,
  /// What we propose in our SYN. The peer's proposal replaces this on
  /// receipt; see cleanroom doc `protocol/20_link_layer.md`.
  pub our_lsp: Lsp,
  /// How often to retransmit the detect marker until the peer responds.
  pub detect_interval: Duration,
  /// Total budget for each handshake stage (Detecting, Negotiating).
  pub handshake_timeout: Duration,
}

impl LinkConfig {
  pub fn new(our_lsp: Lsp) -> Self {
    Self {
      initial_psn: 99,
      our_lsp,
      detect_interval: Duration::from_secs(1),
      handshake_timeout: Duration::from_secs(30),
    }
  }
}

#[derive(Debug, Clone)]
pub enum Iap2Event {
  /// Link reached Established. Carries the peer's negotiated LSP.
  Established(Lsp),
  /// Link is going down for the reason given.
  LinkDown(String),
}

#[derive(Debug, Clone)]
pub enum Iap2Command {
  Disconnect,
}

pub struct Link;

impl Link {
  pub async fn run<S>(
    stream: S,
    config: LinkConfig,
    events_tx: mpsc::Sender<Iap2Event>,
    mut commands_rx: mpsc::Receiver<Iap2Command>,
  ) -> Result<()>
  where
    S: AsyncRead + AsyncWrite + Unpin,
  {
    let (mut reader, mut writer) = tokio::io::split(stream);
    let mut buf = BytesMut::with_capacity(READ_CAPACITY);
    let mut codec = LinkCodec;

    Self::detect_phase(&mut reader, &mut writer, &mut buf, &config).await?;
    let peer_lsp = Self::negotiate_phase(&mut reader, &mut writer, &mut buf, &mut codec, &config).await?;

    if events_tx.send(Iap2Event::Established(peer_lsp)).await.is_err() {
      tracing::debug!("iap2 events receiver dropped before Established could be delivered");
    }
    tracing::info!("iap2 link Established");

    Self::established_phase(
      &mut reader,
      &mut writer,
      &mut buf,
      &mut codec,
      config.initial_psn,
      &events_tx,
      &mut commands_rx,
    )
    .await
  }

  async fn detect_phase<R, W>(reader: &mut R, writer: &mut W, buf: &mut BytesMut, config: &LinkConfig) -> Result<()>
  where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
  {
    tracing::debug!("iap2 link entering Detecting state");
    let mut detect_interval = tokio::time::interval(config.detect_interval);
    detect_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let deadline = tokio::time::sleep(config.handshake_timeout);
    tokio::pin!(deadline);

    loop {
      tokio::select! {
        _ = detect_interval.tick() => {
          tracing::trace!("iap2 sending detect marker");
          writer.write_all(&DETECT_MARKER).await?;
          writer.flush().await?;
        }
        read = reader.read_buf(buf) => {
          let n = read?;
          if n == 0 {
            return Err(Error::PeerDisconnectedDuringHandshake);
          }
          if drain_detect_or_link_start(buf) {
            tracing::debug!("iap2 link detected peer; entering Negotiating");
            return Ok(());
          }
        }
        _ = &mut deadline => {
          return Err(Error::HandshakeTimeout("Detecting"));
        }
      }
    }
  }

  async fn negotiate_phase<R, W>(
    reader: &mut R,
    writer: &mut W,
    buf: &mut BytesMut,
    codec: &mut LinkCodec,
    config: &LinkConfig,
  ) -> Result<Lsp>
  where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
  {
    let our_seq = config.initial_psn;
    let syn = LinkPacket::with_payload(ControlBits::SYN, our_seq, 0, 0, config.our_lsp.encode());
    write_packet(writer, codec, syn).await?;
    tracing::trace!("iap2 sent SYN");

    let deadline = tokio::time::sleep(config.handshake_timeout);
    tokio::pin!(deadline);

    loop {
      if let Some(pkt) = codec.decode(buf)? {
        tracing::trace!("iap2 negotiating: received {:?}", pkt.header);
        if pkt.header.control.contains(ControlBits::RST) {
          return Err(Error::PeerReset);
        }
        if pkt.header.control.contains(ControlBits::SYN) {
          let lsp = Lsp::decode(&pkt.payload)?;
          let ack = pkt.header.seq.wrapping_add(1);
          let standalone_ack = LinkPacket::header_only(ControlBits::ACK, our_seq, ack);
          write_packet(writer, codec, standalone_ack).await?;
          return Ok(lsp);
        }
        return Err(Error::UnexpectedHandshakePacket(pkt.header.control));
      }

      tokio::select! {
        read = reader.read_buf(buf) => {
          let n = read?;
          if n == 0 {
            return Err(Error::PeerDisconnectedDuringHandshake);
          }
        }
        _ = &mut deadline => {
          return Err(Error::HandshakeTimeout("Negotiating"));
        }
      }
    }
  }

  async fn established_phase<R, W>(
    reader: &mut R,
    writer: &mut W,
    buf: &mut BytesMut,
    codec: &mut LinkCodec,
    our_seq: u8,
    events_tx: &mpsc::Sender<Iap2Event>,
    commands_rx: &mut mpsc::Receiver<Iap2Command>,
  ) -> Result<()>
  where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
  {
    loop {
      while let Some(pkt) = codec.decode(buf)? {
        tracing::trace!("iap2 established: received {:?}", pkt.header);
        if pkt.header.control.contains(ControlBits::RST) {
          let _ = events_tx.send(Iap2Event::LinkDown("peer RST".into())).await;
          return Err(Error::PeerReset);
        }
        // Other steady-state traffic is intentionally ignored at the
        // wedge boundary - the auth + identification handlers consume
        // it in the next slice.
      }

      tokio::select! {
        read = reader.read_buf(buf) => {
          let n = read?;
          if n == 0 {
            let _ = events_tx.send(Iap2Event::LinkDown("peer disconnected".into())).await;
            return Err(Error::PeerDisconnected);
          }
        }
        cmd = commands_rx.recv() => {
          match cmd {
            Some(Iap2Command::Disconnect) | None => {
              let rst = LinkPacket::header_only(ControlBits::RST, our_seq, 0);
              if let Err(err) = write_packet(writer, codec, rst).await {
                tracing::warn!("iap2 failed to send RST on disconnect: {:?}", err);
              }
              let _ = events_tx.send(Iap2Event::LinkDown("local disconnect".into())).await;
              return Ok(());
            }
          }
        }
      }
    }
  }
}

/// Returns true once we've seen evidence the peer is past the detect
/// phase. Drains any leading detect markers from `buf`; if a link-packet
/// magic appears in the leading position, treats that as an implicit
/// detect-ack from a peer that skipped its own marker.
fn drain_detect_or_link_start(buf: &mut BytesMut) -> bool {
  use bytes::Buf;
  let mut drained_any = false;
  while buf.starts_with(&DETECT_MARKER) {
    buf.advance(DETECT_MARKER.len());
    drained_any = true;
  }
  drained_any || (buf.len() >= 2 && buf[0..2] == LINK_MAGIC)
}

async fn write_packet<W: AsyncWrite + Unpin>(writer: &mut W, codec: &mut LinkCodec, packet: LinkPacket) -> Result<()> {
  let mut wire = BytesMut::new();
  codec.encode(packet, &mut wire)?;
  writer.write_all(&wire).await?;
  writer.flush().await?;
  Ok(())
}
