use std::net::SocketAddr;
use tokio::net::{TcpListener, TcpStream};

use super::WSResult;

const LISTEN_ADDRESS: &str = "127.0.0.1:8890";

pub struct Server {
  listener: TcpListener,
}

impl Server {
  pub async fn bind() -> WSResult<Self> {
    tracing::info!("binding to address {}", LISTEN_ADDRESS);

    Ok(Self {
      listener: TcpListener::bind(LISTEN_ADDRESS).await?,
    })
  }

  /// cancel-safe
  pub async fn listen(&self) -> WSResult<(TcpStream, SocketAddr)> {
    let tcp_connection = self.listener.accept().await?;
    tracing::info!("new connection from {}", tcp_connection.1);

    Ok(tcp_connection)
  }
}
