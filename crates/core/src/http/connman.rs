use axum::extract::ws::WebSocket;
use dashmap::DashMap;
use libbridgething::{ServerEvent, ServerEventData, ServerEventType};
use std::{net::SocketAddr, sync::Arc};
use tokio::task::{JoinHandle, JoinSet};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::{
  handler::client::{ClientMode, PossibleSendMsg, RecvMsg, RecvMsgData, RecvRx, RecvTx, SendTx},
  state::State,
  stock::StockSendMsg,
};

use super::{WSError, WSResult, connection::Connection};

#[derive(Debug)]
struct ClientData {
  tx: SendTx,
  mode: ClientMode,

  _handle: JoinHandle<()>,
  cancel_token: CancellationToken,
}

pub fn create_client_manager() -> (ClientMan, ClientListener) {
  let (tx, rx) = tokio::sync::mpsc::channel(64);

  let client_man = Arc::new(ClientManager::new(tx));
  let listener = ClientListener::new(rx, client_man.clone());

  (client_man, listener)
}

#[derive(Debug)]
pub struct ClientListener {
  rx: RecvRx,
  client_man: ClientMan,
}

impl ClientListener {
  fn new(rx: RecvRx, client_man: ClientMan) -> Self {
    Self { rx, client_man }
  }

  /// cancel-safe
  pub async fn recv(&mut self) -> WSResult<RecvMsg> {
    let msg = self.rx.recv().await.ok_or(WSError::ChannelClosed)?;
    tracing::trace!("new parsed message from {:?}", msg.from);

    if let RecvMsgData::ChangeMode(mode) = &msg.data {
      self.client_man.change_mode(&msg.from, mode);
    };

    if let RecvMsgData::ConnectionClosed(_, _) = msg.data {
      self.client_man.handle_disconnect(msg.from);
    };

    Ok(msg)
  }
}

pub type ClientMan = Arc<ClientManager>;

#[derive(Debug)]
pub struct ClientManager {
  connections: DashMap<SocketAddr, ClientData>,
  cancel_token: CancellationToken,

  tx: RecvTx,
}

impl ClientManager {
  fn new(tx: RecvTx) -> Self {
    tracing::info!("creating connection manager");

    Self {
      connections: DashMap::new(),
      cancel_token: CancellationToken::new(),

      tx,
    }
  }

  pub fn change_mode(&self, from: &SocketAddr, mode: &ClientMode) {
    if let Some(mut client) = self.connections.get_mut(from) {
      client.mode = *mode;
    }
  }

  pub async fn send(
    &self,
    id: Uuid,
    to: SocketAddr,
    data: impl Into<ServerEventData>,
    meta: ServerEventType,
    stock_msg_id: Option<usize>,
  ) -> WSResult<()> {
    let client = self.connections.get(&to).ok_or(WSError::NotConnected)?;
    let data = data.into();
    tracing::trace!("sending message to {to} with data {:?}", data);

    let msg = ServerEvent { id, data, meta };
    let msg = PossibleSendMsg::from_send_msg(msg, &client.mode, stock_msg_id);

    Ok(client.tx.send(msg).await?)
  }

  pub async fn broadcast(
    &self,
    data: impl Into<ServerEventData> + Clone,
    meta: ServerEventType,
  ) -> Result<(), Vec<WSError>> {
    let data = data.into();

    let msg = ServerEvent {
      id: uuid::Uuid::now_v7(),
      data: data.clone(),
      meta,
    };

    let results: Vec<Result<(), WSError>> = self
      .connections
      .iter()
      .map(|c| {
        let msg = PossibleSendMsg::from_send_msg(msg.clone(), &c.mode, None);
        c.tx.try_send(msg).map_err(WSError::MessageTrySend)
      })
      .collect();

    let errors: Vec<WSError> = results.into_iter().filter_map(Result::err).collect();
    if errors.is_empty() { Ok(()) } else { Err(errors) }
  }

  pub async fn send_stock(&self, to: SocketAddr, data: impl Into<StockSendMsg>) -> WSResult<()> {
    let client = self.connections.get(&to).ok_or(WSError::NotConnected)?;
    if client.mode != ClientMode::Stock {
      tracing::trace!("attempting to send stock message to non-stock device, ignoring...");
      return Ok(());
    }

    let msg = data.into();
    tracing::trace!("sending stock message to {to} with data {:?}", msg);

    Ok(client.tx.send(PossibleSendMsg::Stock(msg)).await?)
  }

  pub async fn broadcast_stock(&self, data: impl Into<StockSendMsg> + Clone) -> Result<(), Vec<WSError>> {
    let msg = data.into();

    let results: Vec<Result<(), WSError>> = self
      .connections
      .iter()
      .map(|c| {
        if c.mode != ClientMode::Stock {
          tracing::trace!("attempting to send stock message to non-stock device, ignoring...");
          return Ok(());
        };

        c.tx
          .try_send(PossibleSendMsg::Stock(msg.clone()))
          .map_err(WSError::MessageTrySend)
      })
      .collect();

    let errors: Vec<WSError> = results.into_iter().filter_map(Result::err).collect();
    if errors.is_empty() { Ok(()) } else { Err(errors) }
  }

  /// NOT cancel-safe
  pub async fn handle_connection(
    &self,
    address: SocketAddr,
    ws: WebSocket,
    mode: ClientMode,
    state: &State,
  ) -> WSResult<()> {
    tracing::debug!("handling accepted websocket connection from {address}");

    let (tx, rx) = tokio::sync::mpsc::channel(16);
    let cancel_token = self.cancel_token.child_token();

    let data = ClientData {
      tx,
      mode,

      _handle: Connection::spawn(address, ws, self.tx.clone(), rx, cancel_token.clone(), mode),
      cancel_token,
    };

    if data.mode == ClientMode::Stock {
      tracing::debug!("new stock connection from {address}");
    } else {
      tracing::debug!("new modern connection from {address}, sending version and gateway status");
      let msg = ServerEvent {
        id: uuid::Uuid::now_v7(),
        data: state.meta.clone().into(),
        meta: ServerEventType::Event,
      };
      let msg = PossibleSendMsg::from_send_msg(msg, &data.mode, None);
      data.tx.send(msg).await?;
      tracing::debug!("sent version info to {address}");

      let status = state.gateway_status().await;
      let msg = ServerEvent {
        id: uuid::Uuid::now_v7(),
        data: status.into(),
        meta: ServerEventType::Event,
      };
      let msg = PossibleSendMsg::from_send_msg(msg, &data.mode, None);
      data.tx.send(msg).await?;
      tracing::debug!("sent gateway status to {address}");
    }

    self.connections.insert(address, data);

    Ok(())
  }

  pub fn handle_disconnect(&self, address: SocketAddr) {
    if let Some((_addr, data)) = self.connections.remove(&address) {
      data.cancel_token.cancel();
      tracing::debug!("removed connection handle for {address}");
    }
  }

  pub async fn _handle_shutdown(self) {
    self.cancel_token.cancel();

    JoinSet::from_iter(self.connections.into_iter().map(|(_, c)| c._handle))
      .join_all()
      .await;
  }
}
