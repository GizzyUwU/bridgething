use crate::{
  bt::Bluetooth,
  msg::{BluetoothRecv, BluetoothSend},
  state::State,
};

use super::{Handler, HandlerResult, MsgHandle};

pub struct BluetoothHandler<'a> {
  handle: MsgHandle<'a>,
  state: &'a mut State,
  bluetooth: &'a mut Bluetooth,
}

impl<'a> BluetoothHandler<'a> {
  pub fn new(handler: Handler<'a>) -> Self {
    Self {
      handle: handler.handle,
      state: handler.state,
      bluetooth: handler.bluetooth,
    }
  }

  pub async fn handle(&mut self, msg: BluetoothRecv) -> HandlerResult {
    tracing::debug!("({}) handling bluetooth message", &self.handle.from);

    match msg {
      BluetoothRecv::List => self.list().await,
      BluetoothRecv::Connect { mac } => self.connect(mac).await,
      BluetoothRecv::Scan => self.scan().await,
      BluetoothRecv::EnableDiscoverable => self.enable_discoverable().await,
      BluetoothRecv::DisableDiscoverable => self.disable_discoverable().await,
      BluetoothRecv::Pair { mac } => self.pair(mac).await,
      BluetoothRecv::Forget { mac } => self.forget(mac).await,
      BluetoothRecv::EnablePAN { mac } => self.enable_pan(mac).await,
      BluetoothRecv::DisablePAN { mac } => self.disable_pan(mac).await,
      BluetoothRecv::SetAlias { name } => self.set_alias(name).await,
    }
  }

  async fn list(&self) -> HandlerResult {
    tracing::debug!("({}) sending list of paired devices", &self.handle.from);

    let devices = self.state.get_devices().to_owned();
    tracing::trace!("({}) devices: {:?}", &self.handle.from, &devices);

    Ok(self.handle.respond(BluetoothSend::PairedDevices(devices)).await?)
  }

  async fn connect(&self, mac: String) -> HandlerResult {
    tracing::debug!("({}) connecting to device with MAC: {}", &self.handle.from, mac);
    Ok(self.bluetooth.connect(&mac)?)
  }

  async fn scan(&self) -> HandlerResult {
    tracing::debug!("({}) scanning for devices", &self.handle.from);
    // Ok(self.handle.respond().await?)
    Ok(())
  }

  async fn enable_discoverable(&self) -> HandlerResult {
    tracing::debug!("({}) enabling discoverable mode", &self.handle.from);
    Ok(self.bluetooth.set_discoverable(true).await?)
  }

  async fn disable_discoverable(&self) -> HandlerResult {
    tracing::debug!("({}) disabling discoverable mode", &self.handle.from);
    Ok(self.bluetooth.set_discoverable(false).await?)
  }

  async fn pair(&self, mac: String) -> HandlerResult {
    tracing::debug!("({}) pairing with device with MAC: {}", &self.handle.from, mac);
    // Ok(self.handle.respond().await?)
    Ok(())
  }

  async fn forget(&mut self, mac: String) -> HandlerResult {
    tracing::debug!("({}) forgetting device with MAC: {}", &self.handle.from, mac);

    self.bluetooth.forget(&mac).await?;
    self.state.remove_device(mac).await?;
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
    Ok(self.bluetooth.set_alias(name).await?)
  }
}
