use std::{
  collections::HashMap,
  net::SocketAddr,
  sync::{Arc, Mutex},
  time::Duration,
};

use axum::extract::ws::WebSocket;
use dashmap::DashMap;
use libbridgething::{
  client::{BridgeToClientMsg, BridgeToClientMsgData, ClientToBridgeMsgData},
  wire::{MsgMeta, RequestError, ResponseMeta, WireCommand, WireError, WireEvent, WireRequest},
};
use tokio::{
  sync::oneshot,
  task::{JoinHandle, JoinSet},
};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::{WSError, WSResult, connection::Connection};
use crate::{
  handler::client::{ClientMode, PossibleSendMsg, RecvMsg, RecvMsgData, RecvRx, RecvTx, SendTx},
  state::State,
  stock::StockSendMsg,
};

/// Default timeout for daemon-initiated typed client requests. Long
/// enough for the webapp's render path, short enough that the requesting
/// daemon code doesn't hang when the webapp goes away.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

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

  /// cancel-safe. Loops past response-meta inbound messages — those
  /// are routed straight to `ClientManager::complete_pending` to resolve
  /// pending daemon-initiated typed requests, and the caller never sees
  /// them. Stray responses (no matching pending request) are warn-logged
  /// and dropped.
  pub async fn recv(&mut self) -> WSResult<RecvMsg> {
    loop {
      let msg = self.rx.recv().await.ok_or(WSError::ChannelClosed)?;
      tracing::trace!("new parsed message from {:?}", msg.from);

      if let RecvMsgData::ChangeMode(mode) = &msg.data {
        self.client_man.change_mode(&msg.from, mode);
      };

      if let RecvMsgData::ConnectionClosed(_, _) = msg.data {
        self.client_man.handle_disconnect(msg.from);
      };

      if let RecvMsgData::Response { request_id, data } = msg.data {
        if !self.client_man.complete_pending(&request_id, data) {
          tracing::warn!(
            "({:?}) stray response-meta message with no matching pending request {request_id}; dropping",
            msg.from
          );
        }
        continue;
      }

      return Ok(msg);
    }
  }
}

pub type ClientMan = Arc<ClientManager>;

#[derive(Debug)]
pub struct ClientManager {
  connections: DashMap<SocketAddr, ClientData>,
  cancel_token: CancellationToken,

  tx: RecvTx,

  /// Pending daemon-initiated typed requests, keyed by the request id
  /// echoed back in `BridgeToClientMsg.meta = Response { request_id }`.
  /// The connection layer drops Response-meta inbound messages into
  /// `complete_pending` instead of normal dispatch.
  pending: Mutex<HashMap<Uuid, oneshot::Sender<ClientToBridgeMsgData>>>,
}

impl ClientManager {
  fn new(tx: RecvTx) -> Self {
    tracing::info!("creating connection manager");

    Self {
      connections: DashMap::new(),
      cancel_token: CancellationToken::new(),

      tx,
      pending: Mutex::new(HashMap::new()),
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
    data: impl Into<BridgeToClientMsgData>,
    meta: MsgMeta,
    stock_msg_id: Option<usize>,
  ) -> WSResult<()> {
    let client = self.connections.get(&to).ok_or(WSError::NotConnected)?;
    let data = data.into();
    tracing::trace!("sending message to {to} with data {:?}", data);

    let msg = BridgeToClientMsg { id, data, meta };
    let msg = PossibleSendMsg::from_send_msg(msg, &client.mode, stock_msg_id);

    Ok(client.tx.send(msg).await?)
  }

  pub async fn broadcast(
    &self,
    data: impl Into<BridgeToClientMsgData> + Clone,
    meta: MsgMeta,
  ) -> Result<(), Vec<WSError>> {
    let data = data.into();

    let msg = BridgeToClientMsg {
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

  /// Send a typed event to one specific webapp connection. Stock-mode
  /// connections get the translated form; modern-mode connections get
  /// the wire shape.
  pub async fn send_event<E: WireEvent<BridgeToClientMsgData>>(&self, to: SocketAddr, event: E) -> WSResult<()> {
    self.send(Uuid::now_v7(), to, event.into(), MsgMeta::Event, None).await
  }

  /// Broadcast a typed event to every connected webapp.
  pub async fn broadcast_event<E: WireEvent<BridgeToClientMsgData> + Clone>(
    &self,
    event: E,
  ) -> Result<(), Vec<WSError>> {
    self.broadcast(event.into(), MsgMeta::Event).await
  }

  /// Send a typed command to one specific webapp connection.
  pub async fn send_command<C: WireCommand<BridgeToClientMsgData>>(&self, to: SocketAddr, cmd: C) -> WSResult<()> {
    self.send(Uuid::now_v7(), to, cmd.into(), MsgMeta::Command, None).await
  }

  /// Broadcast a typed command to every connected webapp.
  pub async fn broadcast_command<C: WireCommand<BridgeToClientMsgData> + Clone>(
    &self,
    cmd: C,
  ) -> Result<(), Vec<WSError>> {
    self.broadcast(cmd.into(), MsgMeta::Command).await
  }

  /// Send a typed request to a specific webapp and await the typed
  /// response. Times out after 10 seconds. The webapp is expected to
  /// echo the request id back in `MsgMeta::Response { request_id }`.
  ///
  /// Domain errors surface as `RequestError::Domain(_)`; protocol
  /// failures (`WireError`, channel close, timeout) as
  /// `RequestError::Protocol(_)`.
  pub async fn request<R>(&self, to: SocketAddr, req: R) -> Result<R::Response, RequestError<R::DomainError>>
  where
    R: WireRequest<Outbound = BridgeToClientMsgData, Inbound = ClientToBridgeMsgData>,
  {
    let id = Uuid::now_v7();
    let (tx, rx) = oneshot::channel();
    self
      .pending
      .lock()
      .expect("pending-request map poisoned")
      .insert(id, tx);

    if let Err(err) = self.send(id, to, req.into(), MsgMeta::Request, None).await {
      self.pending.lock().expect("pending poisoned").remove(&id);
      return Err(RequestError::Protocol(WireError::HandlerFailed {
        reason: format!("send failed: {err:?}"),
      }));
    }

    match tokio::time::timeout(REQUEST_TIMEOUT, rx).await {
      Ok(Ok(data)) => R::extract(data),
      Ok(Err(_)) => {
        self.pending.lock().expect("pending poisoned").remove(&id);
        Err(RequestError::Protocol(WireError::HandlerFailed {
          reason: "response channel closed".into(),
        }))
      }
      Err(_) => {
        self.pending.lock().expect("pending poisoned").remove(&id);
        Err(RequestError::Protocol(WireError::HandlerFailed {
          reason: "request timed out".into(),
        }))
      }
    }
  }

  /// Consume an inbound `Response`-meta message by completing the
  /// matching pending request. Returns `true` if the message was
  /// consumed (caller should not dispatch further); `false` if no
  /// pending request matched.
  pub fn complete_pending(&self, request_id: &Uuid, data: ClientToBridgeMsgData) -> bool {
    let tx = self
      .pending
      .lock()
      .expect("pending-request map poisoned")
      .remove(request_id);
    if let Some(tx) = tx {
      let _ = tx.send(data);
      true
    } else {
      false
    }
  }

  /// Lift a `ResponseMeta` to a `Uuid` request id. Convenience for
  /// the connection layer which holds the meta as a value.
  pub fn complete_pending_meta(&self, meta: &ResponseMeta, data: ClientToBridgeMsgData) -> bool {
    self.complete_pending(&meta.request_id, data)
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
      tracing::debug!("new modern connection from {address}");
    }
    let _ = state;

    let synthesize_change_mode = data.mode == ClientMode::Stock;
    self.connections.insert(address, data);

    // Stock-port connections start in Stock mode without the upgrade
    // path that fires ChangeMode in `connection.rs::handle_text`,
    // so they never trigger the handler that broadcasts current
    // bond/connection state. Synthesize one here, after insertion,
    // so the broadcast that follows reaches the new connection.
    if synthesize_change_mode {
      let msg = RecvMsg {
        id: uuid::Uuid::now_v7(),
        from: address,
        data: RecvMsgData::ChangeMode(ClientMode::Stock),
        stock_msg_id: None,
      };
      if let Err(err) = self.tx.send(msg).await {
        tracing::error!("failed to fire synthetic ChangeMode for {address}: {:?}", err);
      }
    }

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
