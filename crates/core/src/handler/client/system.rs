use libbridgething::client::{ClientToBridgeSystemMsg, RequestVersion};

use super::{HandlerResult, MsgHandle};
use crate::systemd::power;

pub struct SystemHandler {
  handle: MsgHandle,
}

impl SystemHandler {
  pub fn new(handle: MsgHandle) -> Self {
    Self { handle }
  }

  pub async fn handle(&mut self, msg: ClientToBridgeSystemMsg) -> HandlerResult {
    tracing::debug!("({}) handling system message", &self.handle.from);

    match msg {
      ClientToBridgeSystemMsg::VersionRequest => self.version_request().await,
      ClientToBridgeSystemMsg::DiagnosticsGet => Ok(self.handle.unimplemented("system.diagnosticsGet").await?),
      ClientToBridgeSystemMsg::LogsTail(_) => Ok(self.handle.unimplemented("system.logsTail").await?),
      ClientToBridgeSystemMsg::LogsSubscribe(_) => Ok(self.handle.unimplemented("system.logsSubscribe").await?),
      ClientToBridgeSystemMsg::LogsUnsubscribe(_) => Ok(self.handle.unimplemented("system.logsUnsubscribe").await?),
      ClientToBridgeSystemMsg::Reboot => self.reboot().await,
      ClientToBridgeSystemMsg::PowerOff => self.power_off().await,
      ClientToBridgeSystemMsg::FactoryReset => self.factory_reset().await,
    }
  }

  async fn version_request(&self) -> HandlerResult {
    Ok(
      self
        .handle
        .respond_to::<RequestVersion>(self.handle.state.meta.clone().into())
        .await?,
    )
  }

  async fn reboot(&self) -> HandlerResult {
    if let Err(err) = power::reboot().await {
      tracing::error!("reboot failed: {err}");
    }
    Ok(())
  }

  async fn power_off(&self) -> HandlerResult {
    if let Err(err) = power::power_off().await {
      tracing::error!("power_off failed: {err}");
    }
    Ok(())
  }

  async fn factory_reset(&mut self) -> HandlerResult {
    if let Err(err) = self.handle.bluetooth.profile_man.reset().await {
      tracing::error!("error resetting bluetooth devices: {:?}", err);
    }

    if let Err(err) = self.handle.state.reset().await {
      tracing::error!("error resetting state: {:?}", err);
    }

    self.reboot().await?;

    Ok(())
  }
}
