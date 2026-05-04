use libbridgething::{
  client::{self, BridgeToClientNetMsgEvent},
  gateway::{GatewayToBridgeNetMsg, NetWsClosed, NetWsErrorEvent, NetWsMessage},
};

use super::{HandlerResult, MsgHandle};

pub struct NetHandler {
  handle: MsgHandle,
}

impl NetHandler {
  pub fn new(handle: MsgHandle) -> Self {
    Self { handle }
  }

  pub async fn handle(self, msg: GatewayToBridgeNetMsg) -> HandlerResult {
    match msg {
      // typed responses are intercepted by gateway_man.complete_pending
      // before reaching the handler. anything that arrives here is a stray.
      GatewayToBridgeNetMsg::FetchReply(_)
      | GatewayToBridgeNetMsg::FetchErrorReply(_)
      | GatewayToBridgeNetMsg::WsOpenReply(_)
      | GatewayToBridgeNetMsg::WsErrorReply(_) => {
        tracing::warn!(
          "({:?}) stray response-shape Net arrival with no matching pending request; dropping",
          self.handle.address,
        );
        Ok(())
      }
      GatewayToBridgeNetMsg::WsMessage(msg) => self.route_ws_message(msg).await,
      GatewayToBridgeNetMsg::WsClosed(msg) => self.route_ws_closed(msg).await,
      GatewayToBridgeNetMsg::WsErrorEvent(msg) => self.route_ws_error(msg).await,
      GatewayToBridgeNetMsg::StreamBegin(begin) => {
        self
          .route_stream(begin.stream_id, false, BridgeToClientNetMsgEvent::StreamBegin(begin))
          .await
      }
      GatewayToBridgeNetMsg::StreamChunk(chunk) => {
        self
          .route_stream(chunk.stream_id, false, BridgeToClientNetMsgEvent::StreamChunk(chunk))
          .await
      }
      GatewayToBridgeNetMsg::StreamEnd(end) => {
        self
          .route_stream(end.stream_id, true, BridgeToClientNetMsgEvent::StreamEnd(end))
          .await
      }
      GatewayToBridgeNetMsg::StreamError(error) => {
        self
          .route_stream(error.stream_id, true, BridgeToClientNetMsgEvent::StreamError(error))
          .await
      }
    }
  }

  async fn route_ws_message(self, msg: NetWsMessage) -> HandlerResult {
    let Some(owner) = self.handle.state.ws_routes.lookup(msg.connection_id) else {
      tracing::trace!(connection_id = %msg.connection_id, "ws message for unknown connection; dropping");
      return Ok(());
    };
    let event = BridgeToClientNetMsgEvent::WsMessage(client::NetWsMessage {
      connection_id: msg.connection_id,
      frame: msg.frame,
    });
    if let Err(err) = self.handle.state.client_man.send_event(owner, event).await {
      tracing::warn!(?err, "failed to forward ws message to webapp");
    }
    Ok(())
  }

  async fn route_ws_closed(self, msg: NetWsClosed) -> HandlerResult {
    let Some(owner) = self.handle.state.ws_routes.drop_id(msg.connection_id) else {
      tracing::trace!(connection_id = %msg.connection_id, "ws closed for unknown connection; dropping");
      return Ok(());
    };
    let event = BridgeToClientNetMsgEvent::WsClosed(client::NetWsClosed {
      connection_id: msg.connection_id,
      code: msg.code,
      reason: msg.reason,
    });
    if let Err(err) = self.handle.state.client_man.send_event(owner, event).await {
      tracing::warn!(?err, "failed to forward ws closed to webapp");
    }
    Ok(())
  }

  async fn route_ws_error(self, msg: NetWsErrorEvent) -> HandlerResult {
    let Some(owner) = self.handle.state.ws_routes.drop_id(msg.connection_id) else {
      tracing::trace!(connection_id = %msg.connection_id, "ws error for unknown connection; dropping");
      return Ok(());
    };
    let event = BridgeToClientNetMsgEvent::WsErrorEvent(client::NetWsErrorEvent {
      connection_id: msg.connection_id,
      error: msg.error,
    });
    if let Err(err) = self.handle.state.client_man.send_event(owner, event).await {
      tracing::warn!(?err, "failed to forward ws error to webapp");
    }
    Ok(())
  }

  async fn route_stream(
    self,
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
    if let Err(err) = self.handle.state.client_man.send_event(owner, event).await {
      tracing::warn!(?err, "failed to forward stream event to webapp");
    }
    Ok(())
  }
}
