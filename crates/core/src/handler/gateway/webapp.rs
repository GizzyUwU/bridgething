use libbridgething::{
  ConfigEntry, ConfigField,
  client::{BridgeToClientConfigMsgEvent, ConfigChanged},
  gateway::{
    GatewayToBridgeWebappMsgRequest, GetActiveWebapp, ListWebapps, WebappActive, WebappConfigAck, WebappConfigDelete,
    WebappConfigGet, WebappConfigGetReply, WebappConfigList, WebappConfigListReply, WebappConfigSet, WebappError,
    WebappIcon, WebappIconReply, WebappInstall, WebappList, WebappSwitchTo, WebappUninstall,
  },
};
use uuid::Uuid;

use super::{HandlerResult, MsgHandle};
use crate::{chrome::ChromeCommand, state::InstallError};

const KIOSK_HOME_URL: &str = "http://127.0.0.1:8891/";

#[derive(Debug)]
pub struct WebappHandler {
  handle: MsgHandle,
}

impl WebappHandler {
  pub fn new(handle: MsgHandle) -> Self {
    Self { handle }
  }

  pub async fn handle(&mut self, msg: GatewayToBridgeWebappMsgRequest) -> HandlerResult {
    tracing::debug!("({:?}) handling webapp message", &self.handle.address);

    match msg {
      GatewayToBridgeWebappMsgRequest::List => self.list().await,
      GatewayToBridgeWebappMsgRequest::GetActive => self.get_active().await,
      GatewayToBridgeWebappMsgRequest::SwitchTo(req) => self.switch_to(req).await,
      GatewayToBridgeWebappMsgRequest::Install(req) => self.install(req).await,
      GatewayToBridgeWebappMsgRequest::Uninstall(req) => self.uninstall(req).await,
      GatewayToBridgeWebappMsgRequest::Icon(req) => self.icon(req).await,
      GatewayToBridgeWebappMsgRequest::ConfigGet(req) => self.config_get(req).await,
      GatewayToBridgeWebappMsgRequest::ConfigList(req) => self.config_list(req).await,
      GatewayToBridgeWebappMsgRequest::ConfigSet(req) => self.config_set(req).await,
      GatewayToBridgeWebappMsgRequest::ConfigDelete(req) => self.config_delete(req).await,
    }
  }

  async fn list(&self) -> HandlerResult {
    let webapps = self.handle.state.webapps.list().await;
    self.handle.respond_to::<ListWebapps>(WebappList { webapps }).await;
    Ok(())
  }

  async fn get_active(&self) -> HandlerResult {
    let active = active_payload(&self.handle).await?;
    self.handle.respond_to::<GetActiveWebapp>(active).await;
    Ok(())
  }

  async fn switch_to(&self, req: WebappSwitchTo) -> HandlerResult {
    let WebappSwitchTo { id } = req;
    if self.handle.state.webapps.resolve(id).await.is_none() {
      tracing::debug!(
        "({:?}) webapp {id} not in registry; rescanning disk before refusing",
        &self.handle.address
      );
      self.handle.state.webapps.rescan().await;
    }
    if self.handle.state.webapps.resolve(id).await.is_none() {
      tracing::warn!("({:?}) refusing switch to unknown webapp {id}", &self.handle.address);
      self
        .handle
        .respond_err::<WebappSwitchTo>(WebappError::WebappNotFound { id: id.to_string() })
        .await;
      return Ok(());
    }

    self.handle.state.set_active_webapp(id).await?;
    self.reload_kiosk().await;
    let active = active_payload(&self.handle).await?;
    self.handle.respond_to::<WebappSwitchTo>(active).await;
    Ok(())
  }

  async fn install(&self, req: WebappInstall) -> HandlerResult {
    let WebappInstall { archive } = req;
    match self.handle.state.webapps.install(archive).await {
      Ok(info) => {
        if let Some(manifest) = self.handle.state.webapps.manifest(info.id).await
          && let Err(e) = self.handle.state.kv.seed_config_defaults(&manifest).await
        {
          tracing::warn!("config-default seed failed for {}: {:?}", info.id, e);
        }
        self.handle.respond_to::<WebappInstall>(info).await;
      }
      Err(InstallError::Validation(reason)) => {
        self
          .handle
          .respond_err::<WebappInstall>(WebappError::InstallFailed { reason })
          .await;
      }
      Err(InstallError::Io(e)) => {
        self
          .handle
          .respond_err::<WebappInstall>(WebappError::InstallFailed {
            reason: format!("io error: {e}"),
          })
          .await;
      }
    }
    Ok(())
  }

  async fn uninstall(&self, req: WebappUninstall) -> HandlerResult {
    let WebappUninstall { id } = req;
    if self.handle.state.webapps.is_builtin(id).await {
      tracing::warn!("({:?}) refusing uninstall of builtin webapp {id}", &self.handle.address);
      self
        .handle
        .respond_err::<WebappUninstall>(WebappError::CannotUninstallBuiltin { id: id.to_string() })
        .await;
      return Ok(());
    }

    let removed = self.handle.state.webapps.uninstall(id).await?;
    if removed {
      self.handle.state.kv.webapp_purge(id).await?;
    } else {
      tracing::debug!(
        "({:?}) webapp {id} was not installed; nothing to do",
        &self.handle.address
      );
    }

    let active = self.handle.state.active_webapp().await?;
    if active == Some(id) {
      if let Some(fallback) = self.handle.state.webapps.default_id().await {
        tracing::info!("active webapp {id} was uninstalled; falling back to {fallback}");
        self.handle.state.set_active_webapp(fallback).await?;
        self.reload_kiosk().await;
      } else {
        tracing::warn!("active webapp {id} was uninstalled and no fallback is available");
      }
    }

    let active = active_payload(&self.handle).await?;
    self.handle.respond_to::<WebappUninstall>(active).await;
    Ok(())
  }

  async fn icon(&self, req: WebappIcon) -> HandlerResult {
    let WebappIcon { id } = req;
    match self.handle.state.webapps.read_icon(id).await {
      Some((bytes, mime)) => {
        self
          .handle
          .respond_to::<WebappIcon>(WebappIconReply { bytes, mime })
          .await;
      }
      None => {
        self
          .handle
          .respond_err::<WebappIcon>(WebappError::IconNotAvailable { id: id.to_string() })
          .await;
      }
    }
    Ok(())
  }

  async fn config_get(&self, req: WebappConfigGet) -> HandlerResult {
    let WebappConfigGet { id, key } = req;
    if self.handle.state.webapps.bundle(id).await.is_none() {
      self
        .handle
        .respond_err::<WebappConfigGet>(WebappError::WebappNotFound { id: id.to_string() })
        .await;
      return Ok(());
    }
    let value = self.handle.state.kv.config_get(id, &key).await?;
    self
      .handle
      .respond_to::<WebappConfigGet>(WebappConfigGetReply { key, value })
      .await;
    Ok(())
  }

  async fn config_list(&self, req: WebappConfigList) -> HandlerResult {
    let WebappConfigList { id } = req;
    if self.handle.state.webapps.bundle(id).await.is_none() {
      self
        .handle
        .respond_err::<WebappConfigList>(WebappError::WebappNotFound { id: id.to_string() })
        .await;
      return Ok(());
    }
    let entries = self
      .handle
      .state
      .kv
      .config_list(id)
      .await?
      .into_iter()
      .map(|(key, value)| ConfigEntry { key, value })
      .collect();
    self
      .handle
      .respond_to::<WebappConfigList>(WebappConfigListReply { entries })
      .await;
    Ok(())
  }

  async fn config_set(&self, req: WebappConfigSet) -> HandlerResult {
    let WebappConfigSet { id, key, value } = req;
    let manifest = match self.handle.state.webapps.manifest(id).await {
      Some(m) => m,
      None => {
        self
          .handle
          .respond_err::<WebappConfigSet>(WebappError::WebappNotFound { id: id.to_string() })
          .await;
        return Ok(());
      }
    };
    let field = match manifest.config.iter().find(|f| f.key() == key) {
      Some(f) => f,
      None => {
        self
          .handle
          .respond_err::<WebappConfigSet>(WebappError::UnknownConfigKey { key })
          .await;
        return Ok(());
      }
    };
    if let Err(reason) = validate_value(field, &value) {
      self
        .handle
        .respond_err::<WebappConfigSet>(WebappError::InvalidConfigValue { key, reason })
        .await;
      return Ok(());
    }

    self.handle.state.kv.config_set(id, &key, value.clone()).await?;
    self.broadcast_config_change(id, &key, Some(value.clone())).await;
    self
      .handle
      .respond_to::<WebappConfigSet>(WebappConfigAck {
        key,
        value: Some(value),
      })
      .await;
    Ok(())
  }

  async fn config_delete(&self, req: WebappConfigDelete) -> HandlerResult {
    let WebappConfigDelete { id, key } = req;
    let manifest = match self.handle.state.webapps.manifest(id).await {
      Some(m) => m,
      None => {
        self
          .handle
          .respond_err::<WebappConfigDelete>(WebappError::WebappNotFound { id: id.to_string() })
          .await;
        return Ok(());
      }
    };
    let field = match manifest.config.iter().find(|f| f.key() == key) {
      Some(f) => f,
      None => {
        self
          .handle
          .respond_err::<WebappConfigDelete>(WebappError::UnknownConfigKey { key })
          .await;
        return Ok(());
      }
    };

    let restored = field.default_as_storage();
    match restored.clone() {
      Some(default) => {
        self.handle.state.kv.config_set(id, &key, default).await?;
      }
      None => {
        self.handle.state.kv.config_delete(id, &key).await?;
      }
    }
    self.broadcast_config_change(id, &key, restored.clone()).await;
    self
      .handle
      .respond_to::<WebappConfigDelete>(WebappConfigAck { key, value: restored })
      .await;
    Ok(())
  }

  async fn broadcast_config_change(&self, id: Uuid, key: &str, value: Option<String>) {
    let active = match self.handle.state.active_webapp().await {
      Ok(Some(active)) => active,
      _ => return,
    };
    if active != id {
      return;
    }
    let event = BridgeToClientConfigMsgEvent::Changed(ConfigChanged {
      key: key.to_string(),
      value,
    });
    if let Err(errs) = self.handle.state.bus.broadcast_event(event).await {
      tracing::debug!("config-change broadcast: {} non-fatal errors", errs.len());
    }
  }

  async fn reload_kiosk(&self) {
    if let Err(e) = self
      .handle
      .state
      .chrome
      .send(ChromeCommand::Navigate(KIOSK_HOME_URL.to_string()))
      .await
    {
      tracing::warn!("failed to reload kiosk after webapp switch: {:?}", e);
    }
  }
}

async fn active_payload(handle: &MsgHandle) -> Result<WebappActive, crate::state::StateError> {
  let id = handle.state.active_webapp().await?;
  let name = match id {
    Some(id) => handle.state.webapps.bundle(id).await.map(|b| b.manifest.name.clone()),
    None => None,
  };
  Ok(WebappActive { id, name })
}

fn validate_value(field: &ConfigField, value: &str) -> Result<(), String> {
  match field {
    ConfigField::String(f) | ConfigField::Secret(f) => {
      let len = value.chars().count() as u32;
      if let Some(min) = f.min_length
        && len < min
      {
        return Err(format!("value shorter than min_length {min}"));
      }
      if let Some(max) = f.max_length
        && len > max
      {
        return Err(format!("value longer than max_length {max}"));
      }
      // pattern enforcement is gateway-side
    }
    ConfigField::Number(f) => {
      let n = value
        .parse::<f64>()
        .map_err(|_| format!("not a valid number: {value}"))?;
      if let Some(min) = f.min
        && n < min
      {
        return Err(format!("value below min {min}"));
      }
      if let Some(max) = f.max
        && n > max
      {
        return Err(format!("value above max {max}"));
      }
    }
    ConfigField::Boolean(_) => {
      if !matches!(value, "true" | "false") {
        return Err(format!("expected true/false, got {value}"));
      }
    }
    ConfigField::Enum(f) => {
      if !f.choices.iter().any(|c| c == value) {
        return Err(format!("not in choices: {value}"));
      }
    }
  }
  Ok(())
}
