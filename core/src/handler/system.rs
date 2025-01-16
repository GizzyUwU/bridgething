use libbridgething::client::ClientSystemCommand;

use crate::{bt::Bluetooth, state::State};

use super::{Handler, HandlerResult, MsgHandle};

pub struct SystemHandler<'a> {
  handle: MsgHandle<'a>,
  state: &'a mut State,
  bluetooth: &'a mut Bluetooth,
}

impl<'a> SystemHandler<'a> {
  pub fn new(handler: Handler<'a>) -> Self {
    Self {
      handle: handler.handle,
      state: handler.state,
      bluetooth: handler.bluetooth,
    }
  }

  pub async fn handle(&mut self, msg: ClientSystemCommand) -> HandlerResult {
    tracing::debug!("({}) handling system message", &self.handle.from);

    match msg {
      ClientSystemCommand::VersionRequest => self.version_request().await,
      ClientSystemCommand::Reboot => self.reboot().await,
      ClientSystemCommand::PowerOff => self.power_off().await,
      ClientSystemCommand::FactoryReset => self.factory_reset().await,
      ClientSystemCommand::PhoneCallAccept { call_id } => self.phone_call_accept(call_id).await,
      ClientSystemCommand::PhoneCallEnd { call_id } => self.phone_call_end(call_id).await,
      ClientSystemCommand::__LegacyStockReturnToSpotify => self.legacy_stock_return_to_spotify().await,
      ClientSystemCommand::__LegacyStockRemoteConfigurationRequest => {
        self.legacy_stock_remote_configuration_request().await
      }
    }
  }

  async fn version_request(&self) -> HandlerResult {
    tracing::debug!("({}) handling version request", &self.handle.from);
    Ok(self.handle.respond(self.state.meta.clone()).await?)
  }

  async fn reboot(&self) -> HandlerResult {
    tracing::debug!("({}) handling reboot request", &self.handle.from);

    #[cfg(not(debug_assertions))]
    tokio::process::Command::new("sh")
      .arg("-c")
      .arg("sudo reboot")
      .spawn()?;

    Ok(())
  }

  async fn power_off(&self) -> HandlerResult {
    tracing::debug!("({}) handling power off request", &self.handle.from);

    #[cfg(not(debug_assertions))]
    tokio::process::Command::new("sh")
      .arg("-c")
      .arg("sudo shutdown now")
      .spawn()?;

    Ok(())
  }

  async fn factory_reset(&mut self) -> HandlerResult {
    tracing::debug!("({}) handling factory reset request", &self.handle.from);

    if let Err(err) = self.bluetooth.reset(self.state).await {
      tracing::error!("error resetting bluetooth devices: {:?}", err);
    }

    if let Err(err) = self.state.reset().await {
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

  async fn legacy_stock_return_to_spotify(&self) -> HandlerResult {
    tracing::debug!("({}) handling legacy stock return to Spotify", &self.handle.from);
    // Ok(self.handle.respond().await?)
    Ok(())
  }

  async fn legacy_stock_remote_configuration_request(&self) -> HandlerResult {
    tracing::debug!(
      "({}) handling legacy stock remote configuration request",
      &self.handle.id
    );
    // Ok(self.handle.respond().await?)
    Ok(())
  }
}
