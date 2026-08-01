use std::{collections::HashMap, sync::Arc, time::Duration};

use anyhow::{Result, anyhow};
use libbridgething::{
  Priority, TunnelAck, TunnelClosed, TunnelData, TunnelError,
  gateway::{
    BridgeToGatewayMsgData, BridgeToGatewayTunnelMsg, GatewayToBridgeMsg, GatewayToBridgeMsgData,
    GatewayToBridgeTunnelMsg, TunnelErrorReply, TunnelOpen, TunnelOpenReply,
  },
  wire::{MsgMeta, ResponseMeta},
};
use tokio::{
  io::{AsyncReadExt, AsyncWriteExt},
  net::TcpStream,
  sync::{Mutex, mpsc},
};
use uuid::Uuid;

use crate::{
  chaos::ChaosConfig,
  conn::{Connection, OutboundFrame},
};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const READ_CHUNK_BYTES: usize = 16 * 1024;
const ACK_INTERVAL_BYTES: u32 = 16 * 1024;

type Outbound = mpsc::Sender<OutboundFrame>;
type Sockets = Arc<Mutex<HashMap<Uuid, mpsc::Sender<Vec<u8>>>>>;

fn event(data: impl Into<GatewayToBridgeMsgData>, priority: Priority) -> OutboundFrame {
  OutboundFrame {
    msg: GatewayToBridgeMsg {
      id: Uuid::now_v7(),
      meta: MsgMeta::Event,
      data: data.into(),
    },
    priority,
  }
}

pub async fn run_serve(url: &str, chaos: ChaosConfig) -> Result<()> {
  let mut conn = Connection::open(url, chaos).await?;
  conn.announce_version().await?;
  tracing::info!("serving tunnels; the device's net.proxy webapps now reach the network through this host");

  let sockets: Sockets = Arc::new(Mutex::new(HashMap::new()));

  while let Some(msg) = conn.inbound_rx.recv().await {
    let BridgeToGatewayMsgData::Tunnel(tunnel) = &msg.data else {
      continue;
    };
    match tunnel {
      BridgeToGatewayTunnelMsg::Open(open) => {
        let open = open.clone();
        let outbound = conn.outbound_tx.clone();
        let sockets = sockets.clone();
        let request_id = msg.id;
        tokio::spawn(async move { open_tunnel(open, request_id, outbound, sockets).await });
      }
      BridgeToGatewayTunnelMsg::Data(data) => {
        let tx = sockets.lock().await.get(&data.tunnel_id).cloned();
        if let Some(tx) = tx {
          let _ = tx.send(data.bytes.to_vec()).await;
        }
      }
      BridgeToGatewayTunnelMsg::Ack(_) => {}
      BridgeToGatewayTunnelMsg::Close(closed) => {
        sockets.lock().await.remove(&closed.tunnel_id);
        tracing::debug!(tunnel_id = %closed.tunnel_id, "daemon closed tunnel");
      }
    }
  }

  Err(anyhow!("gateway connection closed"))
}

async fn open_tunnel(open: TunnelOpen, request_id: Uuid, outbound: Outbound, sockets: Sockets) {
  let target = format!("{}:{}", open.host, open.port);
  let stream = match tokio::time::timeout(CONNECT_TIMEOUT, TcpStream::connect(&target)).await {
    Ok(Ok(stream)) => stream,
    other => {
      let reason = match other {
        Ok(Err(err)) => err.to_string(),
        _ => "connect timed out".into(),
      };
      tracing::debug!(%target, %reason, "tunnel connect failed");
      let _ = outbound
        .send(OutboundFrame::normal(GatewayToBridgeMsg {
          id: Uuid::now_v7(),
          meta: MsgMeta::Response(ResponseMeta { request_id }),
          data: GatewayToBridgeTunnelMsg::ErrorReply(TunnelErrorReply {
            error: TunnelError::ConnectFailed { reason },
          })
          .into(),
        }))
        .await;
      return;
    }
  };

  let tunnel_id = open.tunnel_id;
  let (tx, mut rx) = mpsc::channel::<Vec<u8>>(32);
  sockets.lock().await.insert(tunnel_id, tx);

  if outbound
    .send(OutboundFrame::normal(GatewayToBridgeMsg {
      id: Uuid::now_v7(),
      meta: MsgMeta::Response(ResponseMeta { request_id }),
      data: GatewayToBridgeTunnelMsg::OpenReply(TunnelOpenReply {}).into(),
    }))
    .await
    .is_err()
  {
    return;
  }
  tracing::info!(%target, %tunnel_id, "tunnel open");

  let (mut read_half, mut write_half) = stream.into_split();

  let up_outbound = outbound.clone();
  let up = tokio::spawn(async move {
    let mut buf = vec![0u8; READ_CHUNK_BYTES];
    loop {
      match read_half.read(&mut buf).await {
        Ok(0) | Err(_) => break,
        Ok(n) => {
          let data = TunnelData {
            tunnel_id,
            bytes: buf[..n].to_vec().into(),
          };
          if up_outbound
            .send(event(GatewayToBridgeTunnelMsg::Data(data), Priority::Bulk))
            .await
            .is_err()
          {
            return;
          }
        }
      }
    }
    let _ = up_outbound
      .send(event(
        GatewayToBridgeTunnelMsg::Closed(TunnelClosed {
          tunnel_id,
          reason: None,
        }),
        Priority::Bulk,
      ))
      .await;
  });

  let mut unacked: u32 = 0;
  while let Some(bytes) = rx.recv().await {
    if write_half.write_all(&bytes).await.is_err() {
      break;
    }
    unacked = unacked.saturating_add(bytes.len() as u32);
    if unacked >= ACK_INTERVAL_BYTES {
      let ack = TunnelAck {
        tunnel_id,
        consumed: unacked,
      };
      unacked = 0;
      if outbound
        .send(event(GatewayToBridgeTunnelMsg::Ack(ack), Priority::Normal))
        .await
        .is_err()
      {
        break;
      }
    }
  }

  up.abort();
  sockets.lock().await.remove(&tunnel_id);
  tracing::info!(%tunnel_id, "tunnel closed");
}
