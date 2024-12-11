use bluer::{
  agent::{
    Agent, AgentHandle, AuthorizeService, DisplayPasskey, DisplayPinCode, ReqResult, RequestAuthorization,
    RequestConfirmation,
  },
  Adapter, AdapterEvent, AdapterProperty, Address, Device,
};
use futures::{Stream, StreamExt};

use crate::{
  msg::{BluetoothSend, SendMsgMeta, SystemSend},
  state::{State, StateError},
  ws::{ConnMan, WSError},
};

pub mod avrcp;
pub mod ble;

pub type BluetoothTx = tokio::sync::mpsc::Sender<BluetoothMsg>;
pub type BluetoothRx = tokio::sync::mpsc::Receiver<BluetoothMsg>;

pub struct Bluetooth {
  rx: BluetoothRx,
  stream: Box<dyn Stream<Item = AdapterEvent> + Unpin>,

  device: Option<Device>,

  adapter: Adapter,
  _agent_handle: AgentHandle,
}

impl Bluetooth {
  pub async fn init() -> bluer::Result<Self> {
    tracing::debug!("initializing bluetooth session");

    let session = bluer::Session::new().await?;
    let adapter = session.default_adapter().await?;

    tracing::debug!("attempting to power on adapter");
    adapter.set_powered(true).await?;

    tracing::info!("initialized bluetooth adapter {}", adapter.name());

    tracing::debug!("configuring adapter");
    adapter.set_discoverable_timeout(0).await?;
    adapter.set_pairable_timeout(0).await?;
    adapter.set_pairable(true).await?;

    let (tx, rx) = tokio::sync::mpsc::channel(16);
    let _agent_handle = build_agent(&session, tx).await?;

    #[cfg(debug_assertions)]
    debug_query_adapter(&adapter).await?;

    Ok(Self {
      rx,
      stream: Box::new(adapter.events().await?),

      device: None,

      adapter,
      _agent_handle,
    })
  }

  /// cancel-safe
  pub async fn listen(&mut self) -> BluetoothMsg {
    tokio::select! {
      Some(msg) = self.rx.recv() => {
        msg
      },
      Some(msg) = self.stream.next() => {
        msg.into()
      },
    }
  }

  pub async fn handle_msg(
    &mut self,
    conn_man: &mut ConnMan,
    state: &mut State,
    msg: BluetoothMsg,
  ) -> Result<(), BluetoothError> {
    match msg {
      // auth/pairing
      BluetoothMsg::AuthRequest { mac } => {
        tracing::info!("bluetooth auth request from mac address: {:?}", &mac);
        Ok(())
      }
      BluetoothMsg::ServiceAuthRequest { mac, service } => {
        tracing::info!(
          "bluetooth service auth request from mac address {:?} to service: {:?}",
          &mac,
          &service
        );
        Ok(())
      }
      BluetoothMsg::PinCode { mac, pin } => {
        tracing::info!(
          "bluetooth device with mac address {:?} pairing pincode: {:?}",
          &mac,
          &pin
        );

        conn_man
          .broadcast(
            BluetoothSend::Pin {
              mac: mac.to_string(),
              name: mac.to_string(),
              pin: pin.to_owned(),
            },
            SendMsgMeta::Info,
          )
          .await?;

        Ok(())
      }

      // adapter
      BluetoothMsg::DeviceAdded { mac } => {
        tracing::info!("bluetooth device added with mac address: {:?}", &mac);
        let just_connected = self.handle_device(mac).await?;

        if let Some(device) = &self.device {
          if just_connected {
            let state_device = crate::msg::Device {
              name: device.name().await?.unwrap_or(mac.to_string()),
              device_type: crate::msg::DeviceType::Unknown,
              mac: mac.to_string(),
              default: true,
            };

            let state_device = match state.get_device(&mac.to_string()) {
              Some(device) => device,
              None => {
                state.add_device(state_device.clone()).await?;

                conn_man
                  .broadcast(BluetoothSend::ParingResult { success: true }, SendMsgMeta::Info)
                  .await?;
                conn_man
                  .broadcast(
                    SystemSend::__LegacyStockSetupStatus("finished".to_owned()),
                    SendMsgMeta::Info,
                  )
                  .await?;

                &state_device
              }
            };

            conn_man
              .broadcast(BluetoothSend::Status { connected: true }, SendMsgMeta::Info)
              .await?;
            conn_man
              .broadcast(
                BluetoothSend::PairedDevices(state.get_devices().clone()),
                SendMsgMeta::Info,
              )
              .await?;
            conn_man
              .broadcast(
                BluetoothSend::ConnectedDevice {
                  name: state_device.name.clone(),
                  mac: state_device.mac.clone(),
                },
                SendMsgMeta::Info,
              )
              .await?;
            conn_man
              .broadcast(
                SystemSend::__LegacyStockRemoteStatus {
                  payload: true,
                  mac: state_device.mac.clone(),
                  phone_type: state_device.device_type.clone(),
                },
                SendMsgMeta::Info,
              )
              .await?;
            conn_man
              .broadcast(
                SystemSend::__LegacyStockTransportStatus { payload: true },
                SendMsgMeta::Info,
              )
              .await?;
          };
        };

        Ok(())
      }
      BluetoothMsg::DeviceRemoved { mac } => {
        tracing::info!("bluetooth device removed with mac address: {:?}", &mac);

        if self.device.take_if(|d| d.address() == mac).is_some() {
          tracing::info!("current device with mac address {:?} has disconnected!", &mac);

          conn_man
            .broadcast(BluetoothSend::Status { connected: false }, SendMsgMeta::Info)
            .await?;
          conn_man
            .broadcast(
              BluetoothSend::PairedDevices(state.get_devices().clone()),
              SendMsgMeta::Info,
            )
            .await?;
          conn_man
            .broadcast(
              SystemSend::__LegacyStockTransportStatus { payload: false },
              SendMsgMeta::Info,
            )
            .await?;
        }

        Ok(())
      }
      BluetoothMsg::AdapterPropertyChanged(property) => {
        tracing::trace!("adapter property changed: {:?}", &property);
        Ok(())
      }
    }
  }

  /// the returned bool is whether this is a new pairing or not
  async fn handle_device(&mut self, mac: Address) -> bluer::Result<bool> {
    tracing::debug!("setting current bluetooth device to {:?}", &mac);
    let device = self.adapter.device(mac)?;

    #[cfg(debug_assertions)]
    debug_query_device(&device).await?;

    if self.device.is_none() && device.is_paired().await? {
      if !device.is_trusted().await? {
        device.set_trusted(true).await?;
      }

      self.device = Some(device);
      return Ok(true);
    }

    Ok(false)
  }

  pub async fn set_alias(&self, alias: String) -> bluer::Result<()> {
    tracing::debug!("setting bluetooth adapter alias to {:?}", &alias);
    self.adapter.set_alias(alias).await
  }

  pub async fn set_discoverable(&self, discoverable: bool) -> bluer::Result<()> {
    tracing::debug!("setting bluetooth discoverable to {:?}", &discoverable);
    self.adapter.set_discoverable(discoverable).await
  }

  pub async fn connect(&self, mac: String) -> bluer::Result<()> {
    tracing::debug!("attempting to connect to device with mac address {:?}", &mac);
    let device = self.adapter.device(mac.parse()?)?;
    device.connect().await
  }
}

#[derive(Debug)]
pub enum BluetoothMsg {
  // auth/pairing
  AuthRequest { mac: Address },
  ServiceAuthRequest { mac: Address, service: uuid::Uuid },
  PinCode { mac: Address, pin: String },

  // adapter
  DeviceAdded { mac: Address },
  DeviceRemoved { mac: Address },
  AdapterPropertyChanged(AdapterProperty),
}

impl From<AdapterEvent> for BluetoothMsg {
  fn from(event: AdapterEvent) -> Self {
    match event {
      AdapterEvent::DeviceAdded(address) => Self::DeviceAdded { mac: address },
      AdapterEvent::DeviceRemoved(address) => Self::DeviceRemoved { mac: address },
      AdapterEvent::PropertyChanged(property) => Self::AdapterPropertyChanged(property),
    }
  }
}

#[derive(Debug, thiserror::Error)]
pub enum BluetoothError {
  #[error("bluez error: {0}")]
  Bluez(#[from] bluer::Error),
  #[error("websocket error: {0}")]
  WS(#[from] WSError),
  #[error("state error: {0}")]
  State(#[from] StateError),
}

impl From<Vec<WSError>> for BluetoothError {
  fn from(errors: Vec<WSError>) -> Self {
    for error in errors {
      tracing::error!("failed to broadcast message: {:?}", error);
    }

    Self::WS(WSError::BroadcastFailed)
  }
}

async fn request_authorization(tx: BluetoothTx, req: RequestAuthorization) -> ReqResult<()> {
  tracing::info!(
    "pairing authorization requested from device {} on adapter {}",
    &req.device,
    &req.adapter
  );

  if let Err(err) = tx.send(BluetoothMsg::AuthRequest { mac: req.device }).await {
    tracing::error!("failed to send bluetooth msg: {:?}", err);
  }

  Ok(())
}

async fn request_confirmation(tx: BluetoothTx, req: RequestConfirmation) -> ReqResult<()> {
  tracing::info!(
    "pairing confirmation requested from device {} on adapter {} with passkey {}",
    &req.device,
    &req.adapter,
    &req.passkey,
  );

  if let Err(err) = tx
    .send(BluetoothMsg::PinCode {
      mac: req.device,
      pin: format!("\"{:06}\"", req.passkey),
    })
    .await
  {
    tracing::error!("failed to send bluetooth msg: {:?}", err);
  }

  Ok(())
}

async fn authorize_service(tx: BluetoothTx, req: AuthorizeService) -> ReqResult<()> {
  tracing::debug!(
    "service authorization requested from {} on adapter {} for service {}",
    &req.device,
    &req.adapter,
    &req.service
  );

  if let Err(err) = tx
    .send(BluetoothMsg::ServiceAuthRequest {
      mac: req.device,
      service: req.service,
    })
    .await
  {
    tracing::error!("failed to send bluetooth msg: {:?}", err);
  }

  Ok(())
}

async fn display_pin_code(tx: BluetoothTx, req: DisplayPinCode) -> ReqResult<()> {
  tracing::info!(
    "pairing pin code for device {} on {} is \"{}\"",
    &req.device,
    &req.adapter,
    req.pincode
  );

  if let Err(err) = tx
    .send(BluetoothMsg::PinCode {
      mac: req.device,
      pin: req.pincode,
    })
    .await
  {
    tracing::error!("failed to send bluetooth msg: {:?}", err);
  }

  Ok(())
}

async fn display_passkey(tx: BluetoothTx, req: DisplayPasskey) -> ReqResult<()> {
  tracing::info!(
    "pairing passkey for device {} on {} is \"{:06}\"",
    &req.device,
    &req.adapter,
    req.passkey
  );

  // yes i know passkey and pin are different but i do not care
  if let Err(err) = tx
    .send(BluetoothMsg::PinCode {
      mac: req.device,
      pin: format!("\"{:06}\"", req.passkey),
    })
    .await
  {
    tracing::error!("failed to send bluetooth msg: {:?}", err);
  }

  Ok(())
}

async fn build_agent(session: &bluer::Session, tx: BluetoothTx) -> bluer::Result<AgentHandle> {
  // i hate that this requires two clones - thank you borrow checker
  let agent = Agent {
    request_default: true,
    display_pin_code: Some(Box::new({
      let tx = tx.clone();
      move |req| Box::pin(display_pin_code(tx.clone(), req))
    })),
    display_passkey: Some(Box::new({
      let tx = tx.clone();
      move |req| Box::pin(display_passkey(tx.clone(), req))
    })),
    request_confirmation: Some(Box::new({
      let tx = tx.clone();
      move |req| Box::pin(request_confirmation(tx.clone(), req))
    })),
    request_authorization: Some(Box::new({
      let tx = tx.clone();
      move |req| Box::pin(request_authorization(tx.clone(), req))
    })),
    authorize_service: Some(Box::new({
      let tx = tx.clone();
      move |req| Box::pin(authorize_service(tx.clone(), req))
    })),
    ..Default::default()
  };

  session.register_agent(agent).await
}

#[cfg(debug_assertions)]
async fn debug_query_adapter(adapter: &bluer::Adapter) -> bluer::Result<()> {
  println!("Debug Adapter Information:");
  println!("Address:                    {}", adapter.address().await?);
  println!("Address type:               {}", adapter.address_type().await?);
  println!("Friendly name:              {}", adapter.alias().await?);
  println!("Modalias:                   {:?}", adapter.modalias().await?);
  println!("Powered:                    {:?}", adapter.is_powered().await?);
  println!("Discoverabe:                {:?}", adapter.is_discoverable().await?);
  println!("Pairable:                   {:?}", adapter.is_pairable().await?);
  println!("UUIDs:                      {:?}\n", adapter.uuids().await?);
  println!(
    "Active adv. instances:      {}",
    adapter.active_advertising_instances().await?
  );
  println!(
    "Supp.  adv. instances:      {}",
    adapter.supported_advertising_instances().await?
  );
  println!(
    "Supp.  adv. includes:       {:?}",
    adapter.supported_advertising_system_includes().await?
  );
  println!(
    "Adv. capabilites:           {:?}",
    adapter.supported_advertising_capabilities().await?
  );
  println!(
    "Adv. features:              {:?}\n",
    adapter.supported_advertising_features().await?
  );

  println!("Adapter Properties:");
  let props = adapter.all_properties().await?;
  for prop in props {
    println!("Property:                   {:?}", &prop);
  }

  Ok(())
}

#[cfg(debug_assertions)]
async fn debug_query_device(device: &Device) -> bluer::Result<()> {
  println!("    Address type:       {}", device.address_type().await?);
  println!("    Name:               {:?}", device.name().await?);
  println!("    Icon:               {:?}", device.icon().await?);
  println!("    Class:              {:?}", device.class().await?);
  println!(
    "    UUIDs:              {:?}",
    device.uuids().await?.unwrap_or_default()
  );
  println!("    Paired:             {:?}", device.is_paired().await?);
  println!("    Connected:          {:?}", device.is_connected().await?);
  println!("    Trusted:            {:?}", device.is_trusted().await?);
  println!("    Modalias:           {:?}", device.modalias().await?);
  println!("    RSSI:               {:?}", device.rssi().await?);
  println!("    TX power:           {:?}", device.tx_power().await?);
  println!("    Manufacturer data:  {:?}", device.manufacturer_data().await?);
  println!("    Service data:       {:?}", device.service_data().await?);

  let props = device.all_properties().await?;
  for prop in props {
    println!("    {:?}", &prop);
  }
  Ok(())
}
