mod connection;
mod connman;

use axum::{
  Router,
  body::Body,
  extract::{ConnectInfo, FromRequest, Path, State as AxumState, WebSocketUpgrade, ws::WebSocket},
  http::{self, Request},
  response::{AppendHeaders, IntoResponse, Response},
};
pub use connman::{ClientMan, create_client_manager};

use libbridgething::{BRIDGETHING_STOCK_WS_PORT, BRIDGETHING_WS_MODERN_PORT};
use reqwest::StatusCode;
use std::{net::SocketAddr, sync::Arc};
use tokio::net::TcpListener;
use tokio_util::{io::ReaderStream, sync::CancellationToken};

use crate::{
  msg::{ClientMode, PossibleSendMsg},
  state::State as BridgeThingState,
};

type ServerTx = tokio::sync::mpsc::Sender<(WebSocket, SocketAddr, ClientMode)>;
type ServerRx = tokio::sync::mpsc::Receiver<(WebSocket, SocketAddr, ClientMode)>;

pub struct Server {
  rx: ServerRx,
  cancel_token: CancellationToken,

  _stock_handle: tokio::task::JoinHandle<()>,
  _modern_handle: tokio::task::JoinHandle<()>,
}

impl Server {
  pub async fn bind(state: BridgeThingState) -> WSResult<Self> {
    tracing::debug!(
      "binding to ports {} (stock) and {} (modern + file serve)",
      BRIDGETHING_STOCK_WS_PORT,
      BRIDGETHING_WS_MODERN_PORT
    );

    let (tx, rx) = tokio::sync::mpsc::channel(64);
    let cancel_token = CancellationToken::new();

    let stock_app = Router::new()
      .fallback(axum::routing::any(stock_ws_handler))
      .with_state(Arc::new(tx.clone()));
    let modern_app = Router::new()
      .route("/{*path}", axum::routing::any(modern_handler))
      .fallback(axum::routing::any(modern_handler))
      .with_state(Arc::new((state, tx)));

    let stock_listener = TcpListener::bind(format!("127.0.0.1:{}", BRIDGETHING_STOCK_WS_PORT)).await?;
    let modern_listener = TcpListener::bind(format!("127.0.0.1:{}", BRIDGETHING_WS_MODERN_PORT)).await?;
    tracing::info!(
      "listening on ports {} (stock) and {} (modern)",
      BRIDGETHING_STOCK_WS_PORT,
      BRIDGETHING_WS_MODERN_PORT
    );

    let stock_cancel_token = cancel_token.clone();
    let _stock_handle = tokio::spawn(async move {
      tokio::select! {
        _ = axum::serve(stock_listener, stock_app.into_make_service_with_connect_info::<SocketAddr>()) => {
          tracing::error!("FATAL: stock server stopped");
        }
        _ = stock_cancel_token.cancelled() => {
          tracing::debug!("stock server shutting down");
        }
      }
    });

    let modern_cancel_token = cancel_token.clone();
    let _modern_handle = tokio::spawn(async move {
      tokio::select! {
        _ = axum::serve(modern_listener, modern_app.into_make_service_with_connect_info::<SocketAddr>()) => {
          tracing::error!("FATAL: modern server stopped");
        }
        _ = modern_cancel_token.cancelled() => {
          tracing::debug!("modern server shutting down");
        }
      }
    });

    Ok(Self {
      rx,
      cancel_token,

      _stock_handle,
      _modern_handle,
    })
  }

  /// cancel-safe
  pub async fn listen(&mut self) -> WSResult<(WebSocket, SocketAddr, ClientMode)> {
    self.rx.recv().await.ok_or(WSError::ChannelClosed)
  }

  pub async fn shutdown(self) {
    self.cancel_token.cancel();
    if let Err(err) = self._stock_handle.await {
      tracing::error!("failed to shutdown stock server: {:?}", err);
    }
    if let Err(err) = self._modern_handle.await {
      tracing::error!("failed to shutdown modern server: {:?}", err);
    }
  }
}

async fn stock_ws_handler(
  ws: WebSocketUpgrade,
  AxumState(tx): AxumState<Arc<ServerTx>>,
  ConnectInfo(addr): ConnectInfo<SocketAddr>,
) -> impl IntoResponse {
  tracing::info!("new stock port websocket connection from {}", addr);

  let tx = tx.clone();
  ws.on_upgrade(move |socket| async move {
    if let Err(err) = tx.send((socket, addr, ClientMode::Stock)).await {
      tracing::error!("failed to send new connection to server: {:?}", err);
    }
  })
}

async fn modern_handler(
  path: Option<Path<String>>,
  ConnectInfo(addr): ConnectInfo<SocketAddr>,
  AxumState(state): AxumState<Arc<(BridgeThingState, ServerTx)>>,
  req: Request<Body>,
) -> Response {
  if req.headers().contains_key("upgrade") {
    match WebSocketUpgrade::from_request(req, &()).await {
      Ok(ws) => {
        tracing::info!("new modern port websocket connection from {}", addr);
        modern_ws_handler(ws, addr, state).await.into_response()
      }
      Err(err) => {
        tracing::error!("failed to upgrade request to websocket: {:?}", err);
        (StatusCode::BAD_REQUEST, err.body_text()).into_response()
      }
    }
  } else {
    let path = match path {
      Some(Path(path)) if path.is_empty() => "index.html".to_string(),
      Some(Path(path)) => path,
      None => "index.html".to_string(),
    };

    tracing::trace!("got file request for {:?}", path);
    modern_file_handler(state, path).await
  }
}

async fn modern_ws_handler(
  ws: WebSocketUpgrade,
  addr: SocketAddr,
  state: Arc<(BridgeThingState, ServerTx)>,
) -> impl IntoResponse {
  tracing::info!("new modern port websocket connection from {}", addr);

  let tx = state.1.clone();
  ws.on_upgrade(move |socket| async move {
    if let Err(err) = tx.send((socket, addr, ClientMode::Modern)).await {
      tracing::error!("failed to send new connection to server: {:?}", err);
    }
  })
}

async fn modern_file_handler(state: Arc<(BridgeThingState, ServerTx)>, path: String) -> Response {
  tracing::debug!("serving file request for {:?}", path);

  if let Ok((file, mime)) = state.0.fs.get_file(&path).await {
    tracing::debug!("serving file {:?} with content_type {:?}", path, &mime);
    let stream = ReaderStream::new(file);
    let body = Body::from_stream(stream);

    let headers = AppendHeaders([(http::header::CONTENT_TYPE, mime.to_string())]);
    return (StatusCode::OK, headers, body).into_response();
  }

  // TODO: request a non-existent file from the gateway

  (StatusCode::NOT_FOUND, "Not Found").into_response()
}

pub type WSResult<T> = Result<T, WSError>;

#[derive(Debug, thiserror::Error)]
pub enum WSError {
  #[error("failed to bind to port: {0}")]
  Bind(#[from] std::io::Error),
  #[error("websocket error: {0}")]
  Websocket(#[from] axum::Error),
  #[error("requested client to send to is not connected to the server!!")]
  NotConnected,
  #[error("could not send a message to requested client: {0}")]
  MessageSend(#[from] tokio::sync::mpsc::error::SendError<PossibleSendMsg>),
  #[error("could not send a message to requested client: {0}")]
  MessageTrySend(#[from] tokio::sync::mpsc::error::TrySendError<PossibleSendMsg>),
  #[error("channel from connections to server struct has been dropped!!! this is bad.")]
  ChannelClosed,
  #[error("failed to broadcast to all devices. check the logs for more info.")]
  BroadcastFailed,
}
