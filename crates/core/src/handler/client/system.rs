use libbridgething::{client::ClientSystemCommand, server::ServerSystemEvent};

use super::{HandlerResult, MsgHandle};

pub struct SystemHandler {
  handle: MsgHandle,
}

impl SystemHandler {
  pub fn new(handle: MsgHandle) -> Self {
    Self { handle }
  }

  pub async fn handle(&mut self, msg: ClientSystemCommand) -> HandlerResult {
    tracing::debug!("({}) handling system message", &self.handle.from);

    match msg {
      ClientSystemCommand::VersionRequest => self.version_request().await,
      ClientSystemCommand::GatewayStatusRequest => self.gateway_status_request().await,
      ClientSystemCommand::Reboot => self.reboot().await,
      ClientSystemCommand::PowerOff => self.power_off().await,
      ClientSystemCommand::FactoryReset => self.factory_reset().await,
      ClientSystemCommand::PhoneCallAccept { call_id } => self.phone_call_accept(call_id).await,
      ClientSystemCommand::PhoneCallEnd { call_id } => self.phone_call_end(call_id).await,
    }
  }

  async fn version_request(&self) -> HandlerResult {
    tracing::debug!("({}) handling version request", &self.handle.from);
    Ok(self.handle.respond(self.handle.state.meta.clone()).await?)
  }

  async fn gateway_status_request(&self) -> HandlerResult {
    tracing::debug!("({}) handling gateway status request", &self.handle.from);
    Ok(self.handle.respond(self.handle.state.gateway_status().await).await?)
  }

  async fn reboot(&self) -> HandlerResult {
    tracing::debug!("({}) handling reboot request", &self.handle.from);

    #[cfg(not(debug_assertions))]
    let status = tokio::process::Command::new("sh")
      .arg("-c")
      .arg("sudo reboot")
      .status()
      .await?;

    #[cfg(not(debug_assertions))]
    if !status.success() {
      tracing::error!("Failed to reboot: {:?}", status);
    }

    Ok(())
  }

  async fn power_off(&self) -> HandlerResult {
    tracing::debug!("({}) handling power off request", &self.handle.from);

    #[cfg(not(debug_assertions))]
    let status = tokio::process::Command::new("sh")
      .arg("-c")
      .arg("sudo shutdown now")
      .status()
      .await?;

    #[cfg(not(debug_assertions))]
    if !status.success() {
      tracing::error!("Failed to reboot: {:?}", status);
    }

    Ok(())
  }

  async fn factory_reset(&mut self) -> HandlerResult {
    tracing::debug!("({}) handling factory reset request", &self.handle.from);

    if let Err(err) = self.handle.bluetooth.profile_man.reset().await {
      tracing::error!("error resetting bluetooth devices: {:?}", err);
    }

    if let Err(err) = self.handle.state.reset().await {
      tracing::error!("error resetting state: {:?}", err);
    }

    self.reboot().await?;

    Ok(())
  }

  async fn phone_call_accept(&self, call_id: String) -> HandlerResult {
    tracing::debug!("({}) accepting phone call with id {}", &self.handle.from, call_id);
    // Ok(self.handle.respond().await?)
    Ok(())
  }

  async fn phone_call_end(&self, call_id: String) -> HandlerResult {
    tracing::debug!("({}) ending phone call with id {}", &self.handle.from, call_id);
    // Ok(self.handle.respond().await?)
    Ok(())
  }
}
