use std::{
  io,
  net::{IpAddr, SocketAddr},
  time::Duration,
};

use bytes::BytesMut;
use libbridgething::{
  TunnelClosed, TunnelData, TunnelError,
  gateway::{BridgeToGatewayTunnelMsgCommand, TunnelErrorReply, TunnelOpen},
  wire::RequestError,
};
use tokio::{
  io::{AsyncReadExt, AsyncWriteExt},
  net::{TcpListener, TcpStream},
  sync::mpsc,
};
use uuid::Uuid;

use crate::{
  bluetooth::BluetoothMan,
  state::{State, TunnelInbound},
};

const PROXY_LISTEN_ADDR: &str = "127.0.0.1:1080";
const PROXY_PERMISSION: &str = "net.proxy";
const READ_CHUNK_BYTES: usize = 16 * 1024;
const TUNNEL_INBOUND_CAPACITY: usize = 32;
const SOCKS_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);

const SOCKS_VERSION: u8 = 0x05;
const SOCKS_AUTH_NONE: u8 = 0x00;
const SOCKS_AUTH_NO_ACCEPTABLE: u8 = 0xff;
const SOCKS_CMD_CONNECT: u8 = 0x01;
const SOCKS_RESERVED: u8 = 0x00;
const SOCKS_ATYP_IPV4: u8 = 0x01;
const SOCKS_ATYP_DOMAIN: u8 = 0x03;
const SOCKS_ATYP_IPV6: u8 = 0x04;
const SOCKS_REP_OK: u8 = 0x00;
const SOCKS_REP_FAILURE: u8 = 0x01;
const SOCKS_REP_NOT_ALLOWED: u8 = 0x02;
const SOCKS_REP_HOST_UNREACHABLE: u8 = 0x04;
const SOCKS_REP_CMD_NOT_SUPPORTED: u8 = 0x07;

pub async fn spawn(state: State, bluetooth: BluetoothMan) -> io::Result<()> {
  let listener = TcpListener::bind(PROXY_LISTEN_ADDR).await?;
  tracing::info!("SOCKS5 proxy listening on {PROXY_LISTEN_ADDR}");
  tokio::spawn(async move {
    loop {
      match listener.accept().await {
        Ok((stream, peer)) => {
          let state = state.clone();
          let bluetooth = bluetooth.clone();
          tokio::spawn(async move {
            if let Err(err) = handle_session(state, bluetooth, stream, peer).await {
              tracing::trace!(?err, %peer, "SOCKS session ended");
            }
          });
        }
        Err(err) => {
          tracing::warn!(?err, "SOCKS accept failed");
          tokio::time::sleep(Duration::from_millis(100)).await;
        }
      }
    }
  });
  Ok(())
}

async fn handle_session(
  state: State,
  bluetooth: BluetoothMan,
  mut stream: TcpStream,
  peer: SocketAddr,
) -> io::Result<()> {
  let request = match tokio::time::timeout(SOCKS_HANDSHAKE_TIMEOUT, socks_handshake(&mut stream)).await {
    Ok(r) => r?,
    Err(_) => {
      tracing::trace!(%peer, "SOCKS handshake timed out");
      return Ok(());
    }
  };

  if !state.active_webapp_has_permission(PROXY_PERMISSION).await {
    tracing::debug!(%peer, host = %request.host, port = request.port, "SOCKS request denied: active webapp lacks net.proxy permission");
    write_socks_reply(&mut stream, SOCKS_REP_NOT_ALLOWED).await?;
    return Ok(());
  }

  let tunnel_id = Uuid::now_v7();
  let (inbound_tx, inbound_rx) = mpsc::channel(TUNNEL_INBOUND_CAPACITY);
  state.tunnel_routes.register(tunnel_id, inbound_tx);

  let open = TunnelOpen {
    tunnel_id,
    host: request.host.clone(),
    port: request.port,
  };
  let reply_code = match bluetooth.gateway_man.request(None, open).await {
    Ok(_) => SOCKS_REP_OK,
    Err(err) => {
      state.tunnel_routes.drop_id(tunnel_id);
      tracing::debug!(?err, host = %request.host, port = request.port, "TunnelOpen rejected");
      tunnel_error_to_socks_rep(&err)
    }
  };

  write_socks_reply(&mut stream, reply_code).await?;
  if reply_code != SOCKS_REP_OK {
    return Ok(());
  }

  bridge(stream, inbound_rx, bluetooth, state, tunnel_id).await;
  Ok(())
}

#[derive(Debug)]
struct SocksRequest {
  host: String,
  port: u16,
}

async fn socks_handshake(stream: &mut TcpStream) -> io::Result<SocksRequest> {
  let mut header = [0u8; 2];
  stream.read_exact(&mut header).await?;
  if header[0] != SOCKS_VERSION {
    return Err(io::Error::new(io::ErrorKind::InvalidData, "not SOCKS5"));
  }
  let nmethods = header[1] as usize;
  let mut methods = vec![0u8; nmethods];
  stream.read_exact(&mut methods).await?;
  let accepts_none = methods.contains(&SOCKS_AUTH_NONE);
  let chosen = if accepts_none {
    SOCKS_AUTH_NONE
  } else {
    SOCKS_AUTH_NO_ACCEPTABLE
  };
  stream.write_all(&[SOCKS_VERSION, chosen]).await?;
  if !accepts_none {
    return Err(io::Error::new(io::ErrorKind::PermissionDenied, "SOCKS auth required"));
  }

  let mut req_header = [0u8; 4];
  stream.read_exact(&mut req_header).await?;
  if req_header[0] != SOCKS_VERSION {
    return Err(io::Error::new(io::ErrorKind::InvalidData, "bad SOCKS5 request version"));
  }
  if req_header[1] != SOCKS_CMD_CONNECT {
    write_socks_reply(stream, SOCKS_REP_CMD_NOT_SUPPORTED).await?;
    return Err(io::Error::new(io::ErrorKind::InvalidData, "only CONNECT is supported"));
  }
  let host = match req_header[3] {
    SOCKS_ATYP_IPV4 => {
      let mut buf = [0u8; 4];
      stream.read_exact(&mut buf).await?;
      IpAddr::from(buf).to_string()
    }
    SOCKS_ATYP_IPV6 => {
      let mut buf = [0u8; 16];
      stream.read_exact(&mut buf).await?;
      IpAddr::from(buf).to_string()
    }
    SOCKS_ATYP_DOMAIN => {
      let mut len_buf = [0u8; 1];
      stream.read_exact(&mut len_buf).await?;
      let mut buf = vec![0u8; len_buf[0] as usize];
      stream.read_exact(&mut buf).await?;
      String::from_utf8(buf).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?
    }
    other => {
      tracing::trace!(atyp = other, "unsupported SOCKS5 ATYP");
      write_socks_reply(stream, SOCKS_REP_CMD_NOT_SUPPORTED).await?;
      return Err(io::Error::new(io::ErrorKind::InvalidData, "unsupported ATYP"));
    }
  };
  let mut port_buf = [0u8; 2];
  stream.read_exact(&mut port_buf).await?;
  let port = u16::from_be_bytes(port_buf);
  Ok(SocksRequest { host, port })
}

async fn write_socks_reply(stream: &mut TcpStream, rep: u8) -> io::Result<()> {
  let frame = [SOCKS_VERSION, rep, SOCKS_RESERVED, SOCKS_ATYP_IPV4, 0, 0, 0, 0, 0, 0];
  stream.write_all(&frame).await
}

fn tunnel_error_to_socks_rep(err: &RequestError<TunnelErrorReply>) -> u8 {
  match err {
    RequestError::Domain(d) => match &d.error {
      TunnelError::ConnectFailed { .. } => SOCKS_REP_HOST_UNREACHABLE,
      TunnelError::PermissionDenied => SOCKS_REP_NOT_ALLOWED,
      TunnelError::Unavailable => SOCKS_REP_FAILURE,
    },
    RequestError::Protocol(_) | RequestError::ResponseMismatch => SOCKS_REP_FAILURE,
  }
}

async fn bridge(
  stream: TcpStream,
  mut inbound_rx: mpsc::Receiver<TunnelInbound>,
  bluetooth: BluetoothMan,
  state: State,
  tunnel_id: Uuid,
) {
  let (mut socks_read, mut socks_write) = stream.into_split();

  let inbound_tunnel_id = tunnel_id;
  let inbound_task = tokio::spawn(async move {
    while let Some(event) = inbound_rx.recv().await {
      match event {
        TunnelInbound::Data(bytes) => {
          if let Err(err) = socks_write.write_all(&bytes).await {
            tracing::trace!(?err, tunnel_id = %inbound_tunnel_id, "SOCKS write failed; closing tunnel");
            return false;
          }
        }
        TunnelInbound::Closed(reason) => {
          tracing::trace!(?reason, tunnel_id = %inbound_tunnel_id, "tunnel closed by gateway");
          return true;
        }
      }
    }
    false
  });

  let bt_for_outbound = bluetooth.clone();
  let outbound_tunnel_id = tunnel_id;
  let outbound_task = tokio::spawn(async move {
    let mut buf = BytesMut::with_capacity(READ_CHUNK_BYTES);
    loop {
      buf.reserve(READ_CHUNK_BYTES);
      match socks_read.read_buf(&mut buf).await {
        Ok(0) => return,
        Ok(_) => {
          let bytes = buf.split().freeze();
          let cmd = BridgeToGatewayTunnelMsgCommand::Data(TunnelData {
            tunnel_id: outbound_tunnel_id,
            bytes,
          });
          bt_for_outbound.gateway_man.broadcast_command_bulk(cmd).await;
        }
        Err(err) => {
          tracing::trace!(?err, tunnel_id = %outbound_tunnel_id, "SOCKS read failed; closing tunnel");
          return;
        }
      }
    }
  });

  let mut inbound_handle = inbound_task;
  let mut outbound_handle = outbound_task;
  let gateway_initiated = tokio::select! {
    res = &mut inbound_handle => {
      outbound_handle.abort();
      res.unwrap_or(false)
    }
    _ = &mut outbound_handle => {
      inbound_handle.abort();
      false
    }
  };

  state.tunnel_routes.drop_id(tunnel_id);
  if !gateway_initiated {
    let close = BridgeToGatewayTunnelMsgCommand::Close(TunnelClosed {
      tunnel_id,
      reason: None,
    });
    bluetooth.gateway_man.broadcast_command_bulk(close).await;
  }
}
