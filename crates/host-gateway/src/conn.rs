//! WebSocket connection driver. Wraps tokio-tungstenite's stream with
//! `GatewayEndec` framing on the companion side: outbound msgs are
//! `GatewayToBridgeMsg` (encoded), inbound msgs are `BridgeToGatewayMsg`
//! (decoded). Loss/disconnect injection happens here so every caller
//! gets the chaos knobs uniformly.

use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use futures::{SinkExt, StreamExt};
use libbridgething::{
  GatewayCapabilities, GatewayInfo, Priority,
  gateway::{BridgeToGatewayMsg, GatewayToBridgeCapabilitiesMsg, GatewayToBridgeMsg, GatewayToBridgeMsgData},
  protocol::{GatewayEndec, PrioritizedFrame},
  wire::MsgMeta,
};
use tokio::{net::TcpStream, sync::mpsc, task::JoinHandle};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async, tungstenite::Message as WsMessage};
use tokio_util::{
  bytes::BytesMut,
  codec::{Decoder, Encoder},
};

use crate::chaos::ChaosConfig;

pub type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// Outbound message handed to the connection writer.
#[derive(Debug, Clone)]
pub struct OutboundFrame {
  pub msg: GatewayToBridgeMsg,
  pub priority: Priority,
}

impl OutboundFrame {
  pub fn normal(msg: GatewayToBridgeMsg) -> Self {
    Self {
      msg,
      priority: Priority::Normal,
    }
  }
}

pub struct Connection {
  pub outbound_tx: mpsc::Sender<OutboundFrame>,
  pub inbound_rx: mpsc::Receiver<BridgeToGatewayMsg>,
  _writer: JoinHandle<()>,
  _reader: JoinHandle<()>,
  _disconnect: Option<JoinHandle<()>>,
}

impl Connection {
  pub async fn open(url: &str, chaos: ChaosConfig) -> Result<Self> {
    tracing::info!(%url, "connecting to daemon network gateway");
    let (ws, _resp) = connect_async(url)
      .await
      .with_context(|| format!("ws connect failed: {url}"))?;
    let (sink, stream) = ws.split();

    let (outbound_tx, outbound_rx) = mpsc::channel::<OutboundFrame>(64);
    let (inbound_tx, inbound_rx) = mpsc::channel::<BridgeToGatewayMsg>(64);

    let _reader = tokio::spawn(reader_task(stream, inbound_tx));
    let _writer = tokio::spawn(writer_task(sink, outbound_rx, chaos));

    let _disconnect = chaos.inject_disconnect.map(|d| {
      let outbound_tx = outbound_tx.clone();
      tokio::spawn(async move {
        tokio::time::sleep(d).await;
        tracing::warn!("inject-disconnect timer fired - dropping connection");
        drop(outbound_tx);
      })
    });

    Ok(Self {
      outbound_tx,
      inbound_rx,
      _writer,
      _reader,
      _disconnect,
    })
  }

  /// Sends a placeholder `GatewayCapabilities::Announce` after open. The
  /// daemon's capabilities handler upserts the peer into the PeerTracker
  /// from the embedded `GatewayInfo`.
  pub async fn announce_version(&self) -> Result<()> {
    let caps = GatewayCapabilities {
      gateway: GatewayInfo {
        address: String::new(),
        name: "host-gateway".into(),
        os_name: "linux".into(),
        app_name: "host-gateway".into(),
        app_version: env!("CARGO_PKG_VERSION").into(),
        adapter_version: "host-gateway".into(),
        lib_version: env!("CARGO_PKG_VERSION").into(),
        libbridgething_version: format!("v{}", libbridgething::LIBBRIDGETHING_VERSION),
      },
      ..Default::default()
    };
    self
      .outbound_tx
      .send(OutboundFrame::normal(GatewayToBridgeMsg {
        id: uuid::Uuid::now_v7(),
        meta: MsgMeta::Event,
        data: GatewayToBridgeMsgData::Capabilities(GatewayToBridgeCapabilitiesMsg::Announce(caps)),
      }))
      .await
      .map_err(|_| anyhow!("connection writer closed before announce send"))?;
    Ok(())
  }
}

async fn reader_task(mut stream: futures::stream::SplitStream<Ws>, tx: mpsc::Sender<BridgeToGatewayMsg>) {
  let mut decoder = GatewayEndec::default();
  let mut buf = BytesMut::new();
  while let Some(msg) = stream.next().await {
    let msg = match msg {
      Ok(m) => m,
      Err(err) => {
        tracing::warn!(?err, "ws read error - exiting reader");
        break;
      }
    };
    match msg {
      WsMessage::Binary(b) => buf.extend_from_slice(&b),
      WsMessage::Close(_) => {
        tracing::info!("daemon closed the connection");
        break;
      }
      WsMessage::Ping(_) | WsMessage::Pong(_) | WsMessage::Frame(_) => continue,
      WsMessage::Text(_) => {
        tracing::warn!("got Text frame from daemon - ignoring");
        continue;
      }
    }

    loop {
      match decoder.decode(&mut buf) {
        Ok(Some(frame)) => {
          if tx.send(frame.msg).await.is_err() {
            return;
          }
        }
        Ok(None) => break,
        Err(err) => {
          tracing::error!(?err, "decode error - dropping reader");
          return;
        }
      }
    }
  }
}

async fn writer_task(
  mut sink: futures::stream::SplitSink<Ws, WsMessage>,
  mut rx: mpsc::Receiver<OutboundFrame>,
  chaos: ChaosConfig,
) {
  let mut encoder = GatewayEndec::default();
  while let Some(frame) = rx.recv().await {
    if chaos.should_drop() {
      tracing::warn!("inject-loss: dropping outbound frame {:?}", frame.msg.meta);
      continue;
    }
    let mut buf = BytesMut::new();
    if let Err(err) = encoder.encode(
      PrioritizedFrame {
        priority: frame.priority,
        msg: frame.msg,
      },
      &mut buf,
    ) {
      tracing::error!(?err, "failed to encode outbound frame - skipping");
      continue;
    }
    if let Err(err) = sink.send(WsMessage::Binary(buf.freeze())).await {
      tracing::warn!(?err, "ws write error - exiting writer");
      break;
    }
  }
  let _ = sink.send(WsMessage::Close(None)).await;
  tracing::debug!("writer task exiting");
}

pub async fn run_connect(url: &str, chaos: ChaosConfig) -> Result<()> {
  let mut conn = Connection::open(url, chaos).await?;
  conn.announce_version().await?;
  loop {
    match conn.inbound_rx.recv().await {
      Some(msg) => tracing::info!(?msg, "inbound"),
      None => {
        tracing::info!("connection closed - exiting");
        return Ok(());
      }
    }
    // soft idle so the loop doesn't spin if the daemon goes silent
    tokio::time::sleep(Duration::from_millis(1)).await;
  }
}
