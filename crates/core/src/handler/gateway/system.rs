use libbridgething::gateway::{ApplyUpdate, GatewayToBridgeSystemMsgCommand};

use super::handle::MsgHandle;
use crate::{handler::HandlerResult, ota::OtaOrchestrator};

#[derive(Debug)]
pub struct SystemHandler {
  handle: MsgHandle,
  ota: OtaOrchestrator,
}

impl SystemHandler {
  pub fn new(handle: MsgHandle, ota: OtaOrchestrator) -> Self {
    Self { handle, ota }
  }

  pub async fn handle(self, cmd: GatewayToBridgeSystemMsgCommand) -> HandlerResult {
    match cmd {
      GatewayToBridgeSystemMsgCommand::ApplyUpdate(req) => self.apply_update(req).await,
      GatewayToBridgeSystemMsgCommand::CancelUpdate => self.cancel_update().await,
    }
  }

  async fn apply_update(&self, req: ApplyUpdate) -> HandlerResult {
    tracing::info!(
      "({:?}) ApplyUpdate received: asset_id={} sha256={} size={}",
      &self.handle.address,
      req.asset_id,
      req.expected_sha256,
      req.expected_size,
    );
    self.ota.apply(req).await;
    Ok(())
  }

  async fn cancel_update(&self) -> HandlerResult {
    tracing::info!("({:?}) CancelUpdate received", &self.handle.address);
    self.ota.cancel().await;
    Ok(())
  }
}
