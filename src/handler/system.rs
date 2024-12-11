use crate::{msg::SystemRecv, state::State};

use super::{Handler, HandlerResult, MsgHandle};

#[derive(Debug)]
pub struct SystemHandler<'a> {
  handle: MsgHandle<'a>,
  state: &'a mut State,
}

impl<'a> SystemHandler<'a> {
  pub fn new(handler: Handler<'a>) -> Self {
    Self {
      handle: handler.handle,
      state: handler.state,
    }
  }

  pub async fn handle(&self, msg: SystemRecv) -> HandlerResult {
    tracing::debug!("({}) handling system message", &self.handle.id);

    match msg {
      SystemRecv::VersionRequest => self.version_request().await,
      SystemRecv::Reboot => self.reboot().await,
      SystemRecv::PowerOff => self.power_off().await,
      SystemRecv::FactoryReset => self.factory_reset().await,
      SystemRecv::PhoneCallAccept { call_id } => self.phone_call_accept(call_id).await,
      SystemRecv::PhoneCallEnd { call_id } => self.phone_call_end(call_id).await,
      SystemRecv::__LegacyStockReturnToSpotify => self.legacy_stock_return_to_spotify().await,
      SystemRecv::__LegacyStockRemoteConfigurationRequest => self.legacy_stock_remote_configuration_request().await,
    }
  }

  async fn version_request(&self) -> HandlerResult {
    tracing::debug!("({}) handling version request", &self.handle.id);
    // Ok(self.handle.respond().await?)
    Ok(())
  }

  async fn reboot(&self) -> HandlerResult {
    tracing::debug!("({}) handling reboot request", &self.handle.id);
    // Ok(self.handle.respond().await?)
    Ok(())
  }

  async fn power_off(&self) -> HandlerResult {
    tracing::debug!("({}) handling power off request", &self.handle.id);
    // Ok(self.handle.respond().await?)
    Ok(())
  }

  async fn factory_reset(&self) -> HandlerResult {
    tracing::debug!("({}) handling factory reset request", &self.handle.id);
    // Ok(self.handle.respond().await?)
    Ok(())
  }

  async fn phone_call_accept(&self, call_id: String) -> HandlerResult {
    tracing::debug!("({}) accepting phone call with id {}", &self.handle.id, call_id);
    // Ok(self.handle.respond().await?)
    Ok(())
  }

  async fn phone_call_end(&self, call_id: String) -> HandlerResult {
    tracing::debug!("({}) ending phone call with id {}", &self.handle.id, call_id);
    // Ok(self.handle.respond().await?)
    Ok(())
  }

  async fn legacy_stock_return_to_spotify(&self) -> HandlerResult {
    tracing::debug!("({}) handling legacy stock return to Spotify", &self.handle.id);
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
