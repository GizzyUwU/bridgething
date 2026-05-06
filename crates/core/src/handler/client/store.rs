use libbridgething::client::{ClientToBridgeStoreMsgRequest, KVDelete, KVGet, KVPut, StorageResponse};
use uuid::Uuid;

use super::{HandlerResult, MsgHandle};
use crate::stock::StockSetupSend;

#[derive(Debug)]
pub struct StorageHandler {
  handle: MsgHandle,
}

impl StorageHandler {
  pub fn new(handle: MsgHandle) -> Self {
    Self { handle }
  }

  pub async fn handle(&mut self, msg: ClientToBridgeStoreMsgRequest) -> HandlerResult {
    tracing::debug!("({}) handling storage message", &self.handle.from);

    let app_id = self.handle.state.active_webapp().await?.unwrap_or(Uuid::nil());
    match msg {
      ClientToBridgeStoreMsgRequest::Get(KVGet { key }) => self.get(app_id, key).await,
      ClientToBridgeStoreMsgRequest::Put(KVPut { key, value }) => self.put(app_id, key, value).await,
      ClientToBridgeStoreMsgRequest::Delete(KVDelete { key }) => self.delete(app_id, key).await,
    }
  }

  async fn get(&self, app_id: Uuid, key: String) -> HandlerResult {
    tracing::debug!("({}) getting value for key: {}", &self.handle.from, &key);

    let mut value = self.handle.state.kv.data_get(app_id, &key).await?;

    // handle for stock firmware
    if &key == "onboarding_status" {
      tracing::trace!(
        "({}) sending setup status to make stock firmware happy",
        &self.handle.from
      );

      let finished = self.handle.state.devices.last().await?.is_some();
      let payload = if finished { "finished" } else { "" }.to_owned();

      self.handle.send_stock(StockSetupSend::Status { payload }).await?;

      if finished {
        value = Some("finished".to_string());
      }
    }

    Ok(self.handle.respond_to::<KVGet>(StorageResponse { key, value }).await?)
  }

  async fn put(&mut self, app_id: Uuid, key: String, value: String) -> HandlerResult {
    tracing::debug!("({}) putting key: {}, value: {}", &self.handle.from, &key, &value);
    self.handle.state.kv.data_set(app_id, &key, value.clone()).await?;

    Ok(
      self
        .handle
        .respond_to::<KVPut>(StorageResponse {
          key,
          value: Some(value),
        })
        .await?,
    )
  }

  async fn delete(&mut self, app_id: Uuid, key: String) -> HandlerResult {
    tracing::debug!("({}) deleting value for key: {}", &self.handle.from, key);
    self.handle.state.kv.data_delete(app_id, &key).await?;

    Ok(
      self
        .handle
        .respond_to::<KVDelete>(StorageResponse { key, value: None })
        .await?,
    )
  }
}
