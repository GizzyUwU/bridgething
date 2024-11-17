use futures::TryFutureExt;
use std::{collections::HashMap, net::SocketAddr};
use tokio::{
  net::TcpStream,
  task::{JoinHandle, JoinSet},
};
use tokio_util::sync::CancellationToken;
use tokio_websockets::ServerBuilder;

use crate::ws::{connection::Connection, RecvMessageWithMeta, WSError};

use super::{
  message::{AddressedRecvMessage, RecvRx, RecvTx},
  SendMessage, SendTx, WSResult,
};

struct ConnectionData {
  handle: JoinHandle<()>,
  cancel_token: CancellationToken,

  tx: SendTx,
}

pub struct ConnMan {
  connections: HashMap<SocketAddr, ConnectionData>,
  cancel_token: CancellationToken,

  tx: RecvTx,
  rx: RecvRx,
}

impl ConnMan {
  pub fn new() -> Self {
    tracing::info!("creating connection manager");

    let (tx, rx) = tokio::sync::mpsc::channel(64);

    Self {
      connections: HashMap::new(),
      cancel_token: CancellationToken::new(),

      tx,
      rx,
    }
  }

  /// cancel-safe
  pub async fn listen(&mut self) -> WSResult<AddressedRecvMessage> {
    let msg = self.rx.recv().await.ok_or(WSError::ChannelClosed)?;
    tracing::trace!("new parsed message from {:?}", msg.from);

    if let RecvMessageWithMeta::ConnectionClosed(_, _) = msg.data {
      self.handle_disconnect(msg.from);
    };

    Ok(msg)
  }

  pub async fn send(&self, address: SocketAddr, msg: impl Into<SendMessage>) -> WSResult<()> {
    let ConnectionData { tx, .. } = self.connections.get(&address).ok_or(WSError::NotConnected)?;

    Ok(tx.send(msg.into()).await?)
  }

  pub async fn broadcast(&self, msg: impl Into<SendMessage> + Clone) -> Vec<WSResult<()>> {
    futures::future::join_all(
      self
        .connections
        .values()
        .map(|c| c.tx.send(msg.clone().into()).map_err(WSError::MessageSend)),
    )
    .await
  }

  /// NOT cancel-safe
  pub async fn handle_connection(&mut self, address: SocketAddr, stream: TcpStream) -> WSResult<()> {
    let stream = ServerBuilder::new().accept(stream).await?;
    tracing::debug!("accepted stream from {address}");

    let (tx, rx) = tokio::sync::mpsc::channel(16);
    let cancel_token = self.cancel_token.child_token();

    let data = ConnectionData {
      handle: Connection::spawn(address, stream, self.tx.clone(), rx, cancel_token.clone()),
      cancel_token,
      tx,
    };

    self.connections.insert(address, data);

    Ok(())
  }

  pub fn handle_disconnect(&mut self, address: SocketAddr) {
    if let Some(data) = self.connections.remove(&address) {
      data.cancel_token.cancel();
      tracing::debug!("removed connection handle for {address}");
    }
  }

  pub async fn handle_shutdown(&mut self) {
    self.cancel_token.cancel();

    JoinSet::from_iter(self.connections.drain().map(|c| c.1.handle))
      .join_all()
      .await;
  }
}

impl Default for ConnMan {
  fn default() -> Self {
    Self::new()
  }
}
