use libbridgething::{BRIDGETHING_STOCK_WS_PORT, BRIDGETHING_WS_MODERN_PORT};
use std::net::SocketAddr;
use tokio::net::{TcpListener, TcpStream};

use super::WSResult;
use crate::msg::ClientMode;

pub struct Server {
  stock_listener: TcpListener,
  modern_listener: TcpListener,
}

impl Server {
  pub async fn bind() -> WSResult<Self> {
    tracing::info!(
      "binding to ports {} (stock) and {} (modern)",
      BRIDGETHING_STOCK_WS_PORT,
      BRIDGETHING_WS_MODERN_PORT
    );
    let stock_listener = TcpListener::bind(format!("127.0.0.1:{}", BRIDGETHING_STOCK_WS_PORT)).await?;
    let modern_listener = TcpListener::bind(format!("127.0.0.1:{}", BRIDGETHING_WS_MODERN_PORT)).await?;
    Ok(Self {
      stock_listener,
      modern_listener,
    })
  }

  /// cancel-safe
  pub async fn listen(&self) -> WSResult<(TcpStream, SocketAddr, ClientMode)> {
    tokio::select! {
      res = self.stock_listener.accept() => {
        let (stream, addr) = res?;
        tracing::info!("new stock connection from {}", addr);
        Ok((stream, addr, ClientMode::Stock))
      }
      res = self.modern_listener.accept() => {
        let (stream, addr) = res?;
        tracing::info!("new modern connection from {}", addr);
        Ok((stream, addr, ClientMode::Modern))
      }
    }
  }
}
