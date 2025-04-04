use bluer::{
  Session,
  rfcomm::{self, ConnectRequest, Profile, ProfileHandle},
};
use futures::StreamExt;
use libbridgething::BRIDGETHING_PROFILE_UUID;
use tokio::task::JoinHandle;

use crate::state::State;

use super::{BluetoothResult, GatewayRecvTx, GatewaySendRx};

#[derive(Debug)]
pub struct RfcommGateway {
  state: State,
  handle: ProfileHandle,

  tx: GatewayRecvTx,
  rx: GatewaySendRx,
}

impl RfcommGateway {
  pub async fn init(session: &Session, state: State, tx: GatewayRecvTx, rx: GatewaySendRx) -> BluetoothResult<Self> {
    tracing::debug!("creating rfcomm gateway profile");
    let profile = Profile {
      uuid: BRIDGETHING_PROFILE_UUID,
      name: Some("bridgething".to_string()),
      role: Some(rfcomm::Role::Server),
      require_authentication: Some(false),
      require_authorization: Some(false),
      ..Default::default()
    };

    let handle = session.register_profile(profile).await?;

    Ok(Self { state, handle, tx, rx })
  }

  pub fn spawn(mut self) -> JoinHandle<()> {
    tokio::spawn(async move {
      if let Err(err) = self.recv().await {
        tracing::error!("gatt server died: {:?}", err);
      }
    })
  }

  async fn recv(&mut self) -> BluetoothResult<()> {
    tracing::debug!("rfcomm gateway listening for connections");

    loop {
      tokio::select! {
        Some(request) = self.handle.next() => {
          if let Err(err) = self.handle_connect_request(request).await {
            tracing::error!("failed to handle connect request: {:?}", err);
          }
        },
        Some(msg) = self.rx.recv() => {
          tracing::debug!("rfcomm gateway received message: {:?}", msg);
          // Handle the message here
        },
        else => {
          tracing::error!("rfcomm profile handle stream ended - this should not happen");
          return Ok(());
        }
      }
    }
  }

  async fn handle_connect_request(&mut self, request: ConnectRequest) -> BluetoothResult<()> {
    tracing::debug!("rfcomm connect request: {:?}", request);
    let device = request.device();
    let mut stream = request.accept()?;

    tracing::debug!("rfcomm connected to device: {:?}", device);

    Ok(())
  }
}
