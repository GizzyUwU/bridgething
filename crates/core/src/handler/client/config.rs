use libbridgething::{
  ConfigEntry,
  client::{ClientToBridgeConfigMsgRequest, ConfigGet, ConfigGetReply, ConfigList, ConfigListReply},
};
use uuid::Uuid;

use super::{HandlerResult, MsgHandle};

pub struct ConfigHandler {
  handle: MsgHandle,
}

impl ConfigHandler {
  pub fn new(handle: MsgHandle) -> Self {
    Self { handle }
  }

  pub async fn handle(self, msg: ClientToBridgeConfigMsgRequest) -> HandlerResult {
    let app_id = self.handle.state.active_webapp().await?.unwrap_or(Uuid::nil());
    match msg {
      ClientToBridgeConfigMsgRequest::Get(ConfigGet { key }) => {
        let value = self.handle.state.kv.config_get(app_id, &key).await?;
        Ok(
          self
            .handle
            .respond_to::<ConfigGet>(ConfigGetReply { key, value })
            .await?,
        )
      }
      ClientToBridgeConfigMsgRequest::List => {
        let entries = self
          .handle
          .state
          .kv
          .config_list(app_id)
          .await?
          .into_iter()
          .map(|(key, value)| ConfigEntry { key, value })
          .collect();
        Ok(
          self
            .handle
            .respond_to::<ConfigList>(ConfigListReply { entries })
            .await?,
        )
      }
    }
  }
}
