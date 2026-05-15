use libbridgething::gateway::{
  DeviceGetNickname, DeviceNicknameRejected, DeviceNicknameReply, DeviceSetNickname, GatewayToBridgeSystemMsgCommand,
  GatewayToBridgeSystemMsgEvent, GatewayToBridgeSystemMsgRequest, OtaAbandon, OtaAssetRangeChunk, OtaBegin, OtaChunk,
};

use super::handle::MsgHandle;
use crate::{handler::HandlerResult, ota::OtaOrchestrator};

const NICKNAME_MAX_LEN: usize = 64;

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
      GatewayToBridgeSystemMsgRequest::DeviceGetNickname => self.device_get_nickname().await,
      GatewayToBridgeSystemMsgRequest::DeviceSetNickname(req) => self.device_set_nickname(req).await,
    }
  }

  async fn device_get_nickname(self) -> HandlerResult {
    let nickname = self.handle.state.meta.nickname();
    self
      .handle
      .respond_to::<DeviceGetNickname>(DeviceNicknameReply { nickname })
      .await;
    Ok(())
  }

  async fn device_set_nickname(self, req: DeviceSetNickname) -> HandlerResult {
    let trimmed = req.nickname.trim();
    if trimmed.contains('\0') {
      self
        .handle
        .respond_err::<DeviceSetNickname>(DeviceNicknameRejected {
          reason: "nickname contains nul byte".into(),
        })
        .await;
      return Ok(());
    }
    if trimmed.chars().count() > NICKNAME_MAX_LEN {
      self
        .handle
        .respond_err::<DeviceSetNickname>(DeviceNicknameRejected {
          reason: format!("nickname longer than {NICKNAME_MAX_LEN} chars"),
        })
        .await;
      return Ok(());
    }

    let next: Option<String> = if trimmed.is_empty() {
      None
    } else {
      Some(trimmed.to_string())
    };
    self.handle.state.meta.set_nickname(next.clone()).await?;
    self
      .handle
      .respond_to::<DeviceSetNickname>(DeviceNicknameReply { nickname: next })
      .await;
    Ok(())
  }

  async fn ota_begin(self, req: OtaBegin) -> HandlerResult {
    tracing::info!(
      "({:?}) OtaBegin received: update_id={} sha256={} size={}",
      &self.handle.address,
      req.update_id,
      req.expected_sha256,
      req.expected_size,
    );
    let peer = self.handle.address;
    match self.ota.begin(req, peer).await {
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
