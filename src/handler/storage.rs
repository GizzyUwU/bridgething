use crate::{
  msg::{StorageRecv, StorageSend, SystemSend},
  state::State,
};

use super::{Handler, HandlerResult, MsgHandle};

#[derive(Debug)]
pub struct StorageHandler<'a> {
  handle: MsgHandle<'a>,
  state: &'a mut State,
}

impl<'a> StorageHandler<'a> {
  pub fn new(handler: Handler<'a>) -> Self {
    Self {
      handle: handler.handle,
      state: handler.state,
    }
  }

  pub async fn handle(&mut self, msg: StorageRecv) -> HandlerResult {
    tracing::debug!("({}) handling storage message", &self.handle.id);

    match msg {
      StorageRecv::Get { key } => self.get(key).await,
      StorageRecv::Put { key, value } => self.put(key, value).await,
      StorageRecv::Delete { key } => self.delete(key).await,
    }
  }

  async fn get(&self, key: String) -> HandlerResult {
    tracing::debug!("({}) getting value for key: {}", &self.handle.id, &key);
    let value = self.state.get_storage_key(&key);

    // handle for stock firmware
    if &key == "onboarding_status" {
      tracing::debug!(
        "({}) sending dummy setup status to make stock firmware happy",
        &self.handle.id
      );
      self
        .handle
        .send_info(SystemSend::__LegacyStockSetupStatus("".to_owned()))
        .await?;
    }

    Ok(self.handle.respond(StorageSend::Response { key, value }).await?)
  }

  async fn put(&mut self, key: String, value: String) -> HandlerResult {
    tracing::debug!("({}) putting key: {}, value: {}", &self.handle.id, &key, &value);
    self.state.set_storage_key(key.clone(), value.clone()).await?;

    Ok(
      self
        .handle
        .respond(StorageSend::Response {
          key,
          value: Some(value),
        })
        .await?,
    )
  }

  async fn delete(&mut self, key: String) -> HandlerResult {
    tracing::debug!("({}) deleting value for key: {}", &self.handle.id, key);
    self.state.del_storage_key(&key).await?;

    Ok(self.handle.respond(StorageSend::Response { key, value: None }).await?)
  }
}
