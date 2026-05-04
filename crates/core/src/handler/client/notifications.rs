use libbridgething::{
  NotificationError,
  client::{
    BridgeToClientMsgData, ClientToBridgeNotificationsMsg, NotificationInvoke as ClientNotificationInvoke,
    NotificationsErrorReply, NotificationsList, NotificationsListReply,
  },
  gateway::{self, BridgeToGatewayNotificationsMsgCommand, NotificationsListRequest},
  wire::{RequestError, WireRequest},
};

use super::{HandlerResult, MsgHandle};

pub struct NotificationsHandler {
  handle: MsgHandle,
}

impl NotificationsHandler {
  pub fn new(handle: MsgHandle) -> Self {
    Self { handle }
  }

  pub async fn handle(self, msg: ClientToBridgeNotificationsMsg) -> HandlerResult {
    match msg {
      ClientToBridgeNotificationsMsg::List(NotificationsList { page_token }) => self.list(page_token).await,
      ClientToBridgeNotificationsMsg::InvokePositive(invoke) => {
        self
          .forward_command(BridgeToGatewayNotificationsMsgCommand::InvokePositive(to_gateway(
            invoke,
          )))
          .await
      }
      ClientToBridgeNotificationsMsg::InvokeNegative(invoke) => {
        self
          .forward_command(BridgeToGatewayNotificationsMsgCommand::InvokeNegative(to_gateway(
            invoke,
          )))
          .await
      }
    }
  }

  async fn list(self, page_token: Option<String>) -> HandlerResult {
    if !self.has_gateway() {
      return self
        .respond_error::<NotificationsList>(NotificationError::NoGateway)
        .await;
    }
    let outbound = NotificationsListRequest { page_token };
    match self.handle.bluetooth.gateway_man.request_bulk(None, outbound).await {
      Ok(reply) => {
        self
          .handle
          .respond_to::<NotificationsList>(NotificationsListReply { page: reply.page })
          .await?;
      }
      Err(err) => {
        self
          .respond_request_error::<NotificationsList>("notifications.list", err)
          .await?
      }
    }
    Ok(())
  }

  fn has_gateway(&self) -> bool {
    self.handle.state.capabilities.snapshot().gateway.is_some()
  }

  async fn forward_command(self, cmd: BridgeToGatewayNotificationsMsgCommand) -> HandlerResult {
    self.handle.bluetooth.gateway_man.broadcast_command(cmd).await;
    Ok(())
  }

  async fn respond_error<R>(&self, error: NotificationError) -> HandlerResult
  where
    R: WireRequest<Inbound = BridgeToClientMsgData, DomainError = NotificationsErrorReply>,
  {
    self
      .handle
      .respond_err::<R>(NotificationsErrorReply { error })
      .await
      .map_err(Into::into)
  }

  async fn respond_request_error<R>(
    &self,
    verb: &str,
    err: RequestError<gateway::NotificationsErrorReply>,
  ) -> HandlerResult
  where
    R: WireRequest<Inbound = BridgeToClientMsgData, DomainError = NotificationsErrorReply>,
  {
    let error = match err {
      RequestError::Domain(domain) => domain.error,
      RequestError::Protocol(err) => {
        tracing::warn!(?err, "{verb} protocol error");
        NotificationError::ActionRejected {
          reason: format!("{err:?}"),
        }
      }
      RequestError::ResponseMismatch => {
        tracing::error!("{verb} response did not match expected shape");
        NotificationError::ActionRejected {
          reason: "response shape mismatch".into(),
        }
      }
    };
    self.respond_error::<R>(error).await
  }
}

fn to_gateway(invoke: ClientNotificationInvoke) -> gateway::NotificationInvoke {
  gateway::NotificationInvoke { id: invoke.id }
}
