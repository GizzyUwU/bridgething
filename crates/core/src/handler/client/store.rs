use libbridgething::{
  client::{ClientKVStoreCommand, KVDelete, KVGet, KVPut},
  server::StorageResponse,
};

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

  pub async fn handle(&mut self, msg: ClientKVStoreCommand) -> HandlerResult {
    tracing::debug!("({}) handling storage message", &self.handle.from);

    match msg {
      ClientKVStoreCommand::Get { key } => self.get(key).await,
      ClientKVStoreCommand::Put { key, value } => self.put(key, value).await,
      ClientKVStoreCommand::Delete { key } => self.delete(key).await,
    }
  }

  async fn get(&self, key: String) -> HandlerResult {
    tracing::debug!("({}) getting value for key: {}", &self.handle.from, &key);
    #[cfg(debug_assertions)]
    let mut value = self.handle.state.get_storage_key(&key).await;

    #[cfg(not(debug_assertions))]
    let value = self.handle.state.get_storage_key(&key).await;

    // handle for stock firmware
    if &key == "onboarding_status" {
      tracing::trace!(
        "({}) sending setup status to make stock firmware happy",
        &self.handle.from
      );

      let payload = if self.handle.state.last_device().await.is_some() {
        "finished"
      } else {
        ""
      }
      .to_owned();

      self.handle.send_stock(StockSetupSend::Status { payload }).await?;

      #[cfg(debug_assertions)]
      {
        value = Some("finished".to_string());
      }
    }

    Ok(self.handle.respond_to::<KVGet>(StorageResponse { key, value }).await?)
  }

  async fn put(&mut self, key: String, value: String) -> HandlerResult {
    tracing::debug!("({}) putting key: {}, value: {}", &self.handle.from, &key, &value);
    self.handle.state.set_storage_key(key.clone(), value.clone()).await?;

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

  async fn delete(&mut self, key: String) -> HandlerResult {
    tracing::debug!("({}) deleting value for key: {}", &self.handle.from, key);
    self.handle.state.del_storage_key(&key).await?;

    Ok(
      self
        .handle
        .respond_to::<KVDelete>(StorageResponse { key, value: None })
        .await?,
    )
  }
}
