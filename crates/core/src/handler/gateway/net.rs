use libbridgething::{
  StreamBegin, StreamChunk, StreamEnd, StreamError,
  client::{self, BridgeToClientNetMsgEvent},
  gateway::{GatewayToBridgeNetMsgEventDispatch, NetWsClosed, NetWsErrorEvent, NetWsMessage},
};

use super::{HandlerResult, MsgHandle};

pub struct NetHandler {
  handle: MsgHandle,
}

impl NetHandler {
  pub fn new(handle: MsgHandle) -> Self {
    Self { handle }
  }

  async fn route_stream(
    &self,
    stream_id: uuid::Uuid,
    terminal: bool,
    event: BridgeToClientNetMsgEvent,
  ) -> HandlerResult {
    let owner = if terminal {
      self.handle.state.stream_routes.drop_id(stream_id)
    } else {
      self.handle.state.stream_routes.lookup(stream_id)
    };
    let Some(owner) = owner else {
      tracing::trace!(%stream_id, "stream event for unknown stream; dropping");
      return Ok(());
    };
    if let Err(err) = self.handle.state.bus.send_event(owner, event).await {
      tracing::warn!(?err, "failed to forward stream event to webapp");
    }
    Ok(())
  }
}

impl GatewayToBridgeNetMsgEventDispatch for NetHandler {
  type Output = HandlerResult;

  async fn ws_message(&self, params: NetWsMessage) -> HandlerResult {
    let Some(owner) = self.handle.state.ws_routes.lookup(params.connection_id) else {
      tracing::trace!(connection_id = %params.connection_id, "ws message for unknown connection; dropping");
      return Ok(());
    };
    let event = BridgeToClientNetMsgEvent::WsMessage(client::NetWsMessage {
      connection_id: params.connection_id,
      frame: params.frame,
    });
    if let Err(err) = self.handle.state.bus.send_event(owner, event).await {
      tracing::warn!(?err, "failed to forward ws message to webapp");
    }
    Ok(())
  }

  async fn ws_closed(&self, params: NetWsClosed) -> HandlerResult {
    let Some(owner) = self.handle.state.ws_routes.drop_id(params.connection_id) else {
      tracing::trace!(connection_id = %params.connection_id, "ws closed for unknown connection; dropping");
      return Ok(());
    };
    let event = BridgeToClientNetMsgEvent::WsClosed(client::NetWsClosed {
      connection_id: params.connection_id,
      code: params.code,
      reason: params.reason,
    });
    if let Err(err) = self.handle.state.bus.send_event(owner, event).await {
      tracing::warn!(?err, "failed to forward ws closed to webapp");
    }
    Ok(())
  }

  async fn ws_error_event(&self, params: NetWsErrorEvent) -> HandlerResult {
    let Some(owner) = self.handle.state.ws_routes.drop_id(params.connection_id) else {
      tracing::trace!(connection_id = %params.connection_id, "ws error for unknown connection; dropping");
      return Ok(());
    };
    let event = BridgeToClientNetMsgEvent::WsErrorEvent(client::NetWsErrorEvent {
      connection_id: params.connection_id,
      error: params.error,
    });
    if let Err(err) = self.handle.state.bus.send_event(owner, event).await {
      tracing::warn!(?err, "failed to forward ws error to webapp");
    }
    Ok(())
  }

  async fn stream_begin(&self, params: StreamBegin) -> HandlerResult {
    self
      .route_stream(params.stream_id, false, BridgeToClientNetMsgEvent::StreamBegin(params))
      .await
  }

  async fn stream_chunk(&self, params: StreamChunk) -> HandlerResult {
    self
      .route_stream(params.stream_id, false, BridgeToClientNetMsgEvent::StreamChunk(params))
      .await
  }

  async fn stream_end(&self, params: StreamEnd) -> HandlerResult {
    self
      .route_stream(params.stream_id, true, BridgeToClientNetMsgEvent::StreamEnd(params))
      .await
  }

  async fn stream_error(&self, params: StreamError) -> HandlerResult {
    self
      .route_stream(params.stream_id, true, BridgeToClientNetMsgEvent::StreamError(params))
      .await
  }
}
