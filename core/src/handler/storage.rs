use libbridgething::{client::ClientStorageCommand, server::ServerStorageEvent};

use crate::{msg::stock::StockSetupSend, state::State};

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

  pub async fn handle(&mut self, msg: ClientStorageCommand) -> HandlerResult {
    tracing::debug!("({}) handling storage message", &self.handle.from);

    match msg {
      ClientStorageCommand::Get { key } => self.get(key).await,
      ClientStorageCommand::Put { key, value } => self.put(key, value).await,
      ClientStorageCommand::Delete { key } => self.delete(key).await,
    }
  }

  async fn get(&self, key: String) -> HandlerResult {
    tracing::debug!("({}) getting value for key: {}", &self.handle.from, &key);
    #[cfg(debug_assertions)]
    let mut value = self.state.get_storage_key(&key);

    #[cfg(not(debug_assertions))]
    let value = self.state.get_storage_key(&key);

    // handle for stock firmware
    if &key == "onboarding_status" {
      tracing::trace!(
        "({}) sending setup status to make stock firmware happy",
        &self.handle.from
      );

      let payload = if self.state.connected_device.is_some() {
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

    Ok(self.handle.respond(ServerStorageEvent::Response { key, value }).await?)
  }

  async fn put(&mut self, key: String, value: String) -> HandlerResult {
    tracing::debug!("({}) putting key: {}, value: {}", &self.handle.from, &key, &value);
    self.state.set_storage_key(key.clone(), value.clone()).await?;

    Ok(
      self
        .handle
        .respond(ServerStorageEvent::Response {
          key,
          value: Some(value),
        })
        .await?,
    )
  }

  async fn delete(&mut self, key: String) -> HandlerResult {
    tracing::debug!("({}) deleting value for key: {}", &self.handle.from, key);
    self.state.del_storage_key(&key).await?;

    Ok(
      self
        .handle
        .respond(ServerStorageEvent::Response { key, value: None })
        .await?,
    )
  }
}
