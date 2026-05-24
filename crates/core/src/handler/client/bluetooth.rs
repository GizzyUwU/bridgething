use libbridgething::client::{
  ClientToBridgeBluetoothMsgDispatch, ConnectBluetooth, DisablePan, EnablePan, ForgetBluetooth, ListBluetoothDevices,
  PairBluetooth, PairedDevicesMap, SetBluetoothAlias,
};

use super::{HandlerResult, MsgHandle};

pub struct BluetoothHandler {
  handle: MsgHandle,
}

impl BluetoothHandler {
  pub fn new(handle: MsgHandle) -> Self {
    Self { handle }
  }
}

impl ClientToBridgeBluetoothMsgDispatch for BluetoothHandler {
  type Output = HandlerResult;

  async fn list(&self) -> HandlerResult {
    tracing::debug!("({}) sending list of paired devices", &self.handle.from);
    let devices = self.handle.state.devices.list().await?;
    tracing::trace!("({}) devices: {:?}", &self.handle.from, &devices);
    Ok(
      self
        .handle
        .respond_to::<ListBluetoothDevices>(PairedDevicesMap(devices.into_iter().collect()))
        .await?,
    )
  }

  async fn connect(&self, params: ConnectBluetooth) -> HandlerResult {
    let ConnectBluetooth { mac } = params;
    tracing::debug!("({}) connecting to device with MAC: {}", &self.handle.from, mac);
    Ok(self.handle.bluetooth.connect(&mac).await?)
  }

  async fn scan(&self) -> HandlerResult {
    tracing::debug!("({}) scanning for devices", &self.handle.from);
    Ok(())
  }

  async fn enable_discoverable(&self) -> HandlerResult {
    tracing::debug!("({}) enabling discoverable mode", &self.handle.from);
    Ok(
      self
        .handle
        .bluetooth
        .profile_man
        .get()
        .await
        .set_discoverable(true)
        .await?,
    )
  }

  async fn disable_discoverable(&self) -> HandlerResult {
    tracing::debug!("({}) disabling discoverable mode", &self.handle.from);
    Ok(
      self
        .handle
        .bluetooth
        .profile_man
        .get()
        .await
        .set_discoverable(false)
        .await?,
    )
  }

  async fn pair(&self, params: PairBluetooth) -> HandlerResult {
    let PairBluetooth { mac } = params;
    tracing::debug!("({}) pairing with device with MAC: {}", &self.handle.from, mac);
    Ok(())
  }

  async fn forget(&self, params: ForgetBluetooth) -> HandlerResult {
    let ForgetBluetooth { mac } = params;
    tracing::debug!("({}) forgetting device with MAC: {}", &self.handle.from, mac);

    self.handle.bluetooth.profile_man.get().await.forget(&mac).await?;
    self.handle.state.devices.remove(mac).await?;

    let devices = self.handle.state.devices.list().await?;
    self
      .handle
      .respond_to::<ListBluetoothDevices>(PairedDevicesMap(devices.into_iter().collect()))
      .await?;

    Ok(())
  }

  async fn enable_pan(&self, params: EnablePan) -> HandlerResult {
    let EnablePan { mac } = params;
    tracing::debug!("({}) enabling PAN on device with MAC: {}", &self.handle.from, mac);
    Ok(())
  }

  async fn disable_pan(&self, params: DisablePan) -> HandlerResult {
    let DisablePan { mac } = params;
    tracing::debug!("({}) disabling PAN on device with MAC: {}", &self.handle.from, mac);
    Ok(())
  }

  async fn set_alias(&self, params: SetBluetoothAlias) -> HandlerResult {
    let SetBluetoothAlias { name } = params;
    tracing::debug!("({}) setting adapter alias to: {}", &self.handle.from, name);
    Ok(self.handle.bluetooth.profile_man.get().await.set_alias(name).await?)
  }
}
