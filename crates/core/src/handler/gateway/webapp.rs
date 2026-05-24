use libbridgething::{
  ConfigEntry, ConfigField, WebappError,
  client::{BridgeToClientConfigMsgEvent, ConfigChanged},
  gateway::{
    GatewayToBridgeWebappMsgCommandDispatch, GatewayToBridgeWebappMsgEventDispatch,
    GatewayToBridgeWebappMsgRequestDispatch, GetActiveWebapp, ListWebapps, WebappActive, WebappConfigAck,
    WebappConfigDelete, WebappConfigGet, WebappConfigGetReply, WebappConfigList, WebappConfigListReply,
    WebappConfigSet, WebappIcon, WebappIconReply, WebappInstallAbandon, WebappInstallBegin, WebappInstallBeginAck,
    WebappInstallChunk, WebappList, WebappSwitchTo, WebappUninstall,
  },
};
use uuid::Uuid;

use super::{HandlerResult, MsgHandle};
use crate::chrome::ChromeCommand;

const KIOSK_HOME_URL: &str = "http://127.0.0.1:8891/";
const KIOSK_HUB_URL_BASE: &str = "http://127.0.0.1:8891/_hub/";

#[derive(Debug)]
pub struct WebappHandler {
  handle: MsgHandle,
}

impl WebappHandler {
  pub fn new(handle: MsgHandle) -> Self {
    Self { handle }
  }
}

impl GatewayToBridgeWebappMsgRequestDispatch for WebappHandler {
  type Output = HandlerResult;

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

  async fn switch_to(&self, params: WebappSwitchTo) -> HandlerResult {
    let WebappSwitchTo { id } = params;
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

  async fn install_begin(&self, params: WebappInstallBegin) -> HandlerResult {
    tracing::info!(
      "({:?}) WebappInstallBegin install_id={} sha256={} size={}",
      &self.handle.address,
      params.install_id,
      params.expected_sha256,
      params.expected_size,
    );
    match crate::install::install_begin(
      &self.handle.state,
      params.install_id,
      params.expected_sha256,
      params.expected_size,
    )
    .await
    {
      Ok(resume_from_offset) => {
        self
          .handle
          .respond_to::<WebappInstallBegin>(WebappInstallBeginAck { resume_from_offset })
          .await
      }
      Err(err) => self.handle.respond_err::<WebappInstallBegin>(err).await,
    }
    Ok(())
  }

  async fn uninstall(&self, params: WebappUninstall) -> HandlerResult {
    let WebappUninstall { id } = params;
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

  async fn icon(&self, params: WebappIcon) -> HandlerResult {
    let WebappIcon { id } = params;
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

  async fn config_get(&self, params: WebappConfigGet) -> HandlerResult {
    let WebappConfigGet { id, key } = params;
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

  async fn config_list(&self, params: WebappConfigList) -> HandlerResult {
    let WebappConfigList { id } = params;
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

  async fn config_set(&self, params: WebappConfigSet) -> HandlerResult {
    let WebappConfigSet { id, key, value } = params;
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

  async fn config_delete(&self, params: WebappConfigDelete) -> HandlerResult {
    let WebappConfigDelete { id, key } = params;
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
}

impl GatewayToBridgeWebappMsgCommandDispatch for WebappHandler {
  type Output = HandlerResult;

  async fn install_abandon(&self, params: WebappInstallAbandon) -> HandlerResult {
    tracing::info!(
      "({:?}) WebappInstallAbandon install_id={}",
      &self.handle.address,
      params.install_id,
    );
    crate::install::install_abandon(&self.handle.state, params.install_id).await;
    Ok(())
  }
}

impl GatewayToBridgeWebappMsgEventDispatch for WebappHandler {
  type Output = HandlerResult;

  async fn install_chunk(&self, params: WebappInstallChunk) -> HandlerResult {
    tracing::trace!(
      "({:?}) WebappInstallChunk install_id={} offset={} len={} last={}",
      &self.handle.address,
      params.install_id,
      params.offset,
      params.bytes.len(),
      params.last,
    );
    crate::install::accept_install_chunk(
      &self.handle.state,
      &self.handle.bluetooth,
      params.install_id,
      params.offset,
      params.bytes,
      params.last,
    )
    .await;
    Ok(())
  }
}

impl WebappHandler {
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
    let url = navigate_url_for_active(&self.handle.state).await;
    if let Err(e) = self.handle.state.chrome.send(ChromeCommand::Navigate(url)).await {
      tracing::warn!("failed to reload kiosk after webapp switch: {:?}", e);
    }
  }
}

pub async fn navigate_url_for_active(state: &crate::state::State) -> String {
  let Ok(Some(active)) = state.active_webapp().await else {
    return KIOSK_HOME_URL.to_string();
  };
  if active != crate::state::HUB_WEBAPP_ID {
    return KIOSK_HOME_URL.to_string();
  }
  match state.webapps.bundle_hash(crate::state::HUB_WEBAPP_ID).await {
    Some(hash) => format!("{KIOSK_HUB_URL_BASE}{hash}/"),
    None => KIOSK_HOME_URL.to_string(),
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
