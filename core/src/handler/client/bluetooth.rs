use libbridgething::{client::ClientBluetoothCommand, server::ServerBluetoothEvent};

use super::{HandlerResult, MsgHandle};

pub struct BluetoothHandler {
  handle: MsgHandle,
}

impl BluetoothHandler {
  pub fn new(handle: MsgHandle) -> Self {
    Self { handle }
  }

  pub async fn handle(&mut self, msg: ClientBluetoothCommand) -> HandlerResult {
    tracing::debug!("({}) handling bluetooth message", &self.handle.from);

    match msg {
      ClientBluetoothCommand::List => self.list().await,
      ClientBluetoothCommand::Connect { mac } => self.connect(mac).await,
      ClientBluetoothCommand::Scan => self.scan().await,
      ClientBluetoothCommand::EnableDiscoverable => self.enable_discoverable().await,
      ClientBluetoothCommand::DisableDiscoverable => self.disable_discoverable().await,
      ClientBluetoothCommand::Pair { mac } => self.pair(mac).await,
      ClientBluetoothCommand::Forget { mac } => self.forget(mac).await,
      ClientBluetoothCommand::EnablePAN { mac } => self.enable_pan(mac).await,
      ClientBluetoothCommand::DisablePAN { mac } => self.disable_pan(mac).await,
      ClientBluetoothCommand::SetAlias { name } => self.set_alias(name).await,
    }
  }

  async fn list(&self) -> HandlerResult {
    tracing::debug!("({}) sending list of paired devices", &self.handle.from);

    let devices = self.handle.state.get_devices().await;
    tracing::trace!("({}) devices: {:?}", &self.handle.from, &devices);

    Ok(
      self
        .handle
        .respond(ServerBluetoothEvent::PairedDevices(devices.into_iter().collect()))
        .await?,
    )
  }

  async fn connect(&self, mac: String) -> HandlerResult {
    tracing::debug!("({}) connecting to device with MAC: {}", &self.handle.from, mac);
    Ok(self.handle.bluetooth.connect(&mac)?)
  }

  async fn scan(&self) -> HandlerResult {
    tracing::debug!("({}) scanning for devices", &self.handle.from);
    // Ok(self.handle.respond().await?)
    Ok(())
  }

  async fn enable_discoverable(&self) -> HandlerResult {
    tracing::debug!("({}) enabling discoverable mode", &self.handle.from);
    Ok(self.handle.bluetooth.set_discoverable(true).await?)
  }

  async fn disable_discoverable(&self) -> HandlerResult {
    tracing::debug!("({}) disabling discoverable mode", &self.handle.from);
    Ok(self.handle.bluetooth.set_discoverable(false).await?)
  }

  async fn pair(&self, mac: String) -> HandlerResult {
    tracing::debug!("({}) pairing with device with MAC: {}", &self.handle.from, mac);
    // Ok(self.handle.respond().await?)
    Ok(())
  }

  async fn forget(&mut self, mac: String) -> HandlerResult {
    tracing::debug!("({}) forgetting device with MAC: {}", &self.handle.from, mac);

    self.handle.bluetooth.forget(&mac).await?;
    self.handle.state.remove_device(mac).await?;
    self.list().await?;

    // Ok(self.handle.respond().await?)
    Ok(())
  }

  async fn enable_pan(&self, mac: String) -> HandlerResult {
    tracing::debug!("({}) enabling PAN on device with MAC: {}", &self.handle.from, mac);
    // Ok(self.handle.respond().await?)
    Ok(())
  }

  async fn disable_pan(&self, mac: String) -> HandlerResult {
    tracing::debug!("({}) disabling PAN on device with MAC: {}", &self.handle.from, mac);
    // Ok(self.handle.respond().await?)
    Ok(())
  }

  async fn set_alias(&self, name: String) -> HandlerResult {
    tracing::debug!("({}) setting adapter alias to: {}", &self.handle.from, name);
    Ok(self.handle.bluetooth.set_alias(name).await?)
  }
}
