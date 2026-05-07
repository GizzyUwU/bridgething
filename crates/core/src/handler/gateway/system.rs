use libbridgething::gateway::{
  GatewayToBridgeSystemMsgCommand, GatewayToBridgeSystemMsgEvent, GatewayToBridgeSystemMsgRequest, OtaAbandon,
  OtaAssetRangeChunk, OtaBegin, OtaChunk,
};

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

  pub async fn handle_command(self, cmd: GatewayToBridgeSystemMsgCommand) -> HandlerResult {
    match cmd {
      GatewayToBridgeSystemMsgCommand::OtaAbandon(req) => self.ota_abandon(req).await,
      GatewayToBridgeSystemMsgCommand::CancelUpdate => self.cancel_update().await,
    }
  }

  pub async fn handle_event(self, ev: GatewayToBridgeSystemMsgEvent) -> HandlerResult {
    match ev {
      GatewayToBridgeSystemMsgEvent::OtaChunk(chunk) => self.ota_chunk(chunk).await,
      GatewayToBridgeSystemMsgEvent::OtaAssetRangeChunk(chunk) => self.ota_asset_range_chunk(chunk).await,
    }
  }

  pub async fn handle_request(self, req: GatewayToBridgeSystemMsgRequest) -> HandlerResult {
    match req {
      GatewayToBridgeSystemMsgRequest::OtaBegin(begin) => self.ota_begin(begin).await,
    }
  }

  async fn ota_begin(self, req: OtaBegin) -> HandlerResult {
    tracing::info!(
      "({:?}) OtaBegin received: update_id={} sha256={} size={}",
      &self.handle.address,
      req.update_id,
      req.expected_sha256,
      req.expected_size,
    );
    match self.ota.begin(req).await {
      Ok(ack) => self.handle.respond_to::<OtaBegin>(ack).await,
      Err(rej) => self.handle.respond_err::<OtaBegin>(rej).await,
    }
    Ok(())
  }

  async fn ota_chunk(self, chunk: OtaChunk) -> HandlerResult {
    tracing::trace!(
      "({:?}) OtaChunk update_id={} offset={} len={} last={}",
      &self.handle.address,
      chunk.update_id,
      chunk.offset,
      chunk.bytes.len(),
      chunk.last,
    );
    self.ota.chunk(chunk).await;
    Ok(())
  }

  async fn ota_abandon(self, req: OtaAbandon) -> HandlerResult {
    tracing::info!("({:?}) OtaAbandon update_id={}", &self.handle.address, req.update_id);
    self.ota.abandon(req.update_id).await;
    Ok(())
  }

  async fn cancel_update(self) -> HandlerResult {
    tracing::info!("({:?}) CancelUpdate received", &self.handle.address);
    self.ota.cancel().await;
    Ok(())
  }

  async fn ota_asset_range_chunk(self, chunk: OtaAssetRangeChunk) -> HandlerResult {
    tracing::trace!(
      "({:?}) OtaAssetRangeChunk request_id={} part={} offset={} len={} last={}",
      &self.handle.address,
      chunk.request_id,
      chunk.part_index,
      chunk.offset,
      chunk.bytes.len(),
      chunk.last,
    );
    self.ota.asset_range_chunk(chunk).await;
    Ok(())
  }
}
