//! Wall-clock authority. Holds the most recent `TimeInfo` snapshot
//! the daemon has been told about — by the iAP2 control session
//! (`DeviceTimeUpdate`) when an iPhone is connected, by the gateway
//! companion (`GatewayToBridgeTimeMsg::Snapshot`) otherwise. Webapps
//! query through the Time SDK surface; broadcast happens on every
//! apply.

use std::sync::Arc;

use libbridgething::{
  TimeInfo,
  client::{BridgeToClientTimeMsg, TimeSnapshot},
  wire::MsgMeta,
};
use tokio::sync::RwLock;

use crate::net::ClientMan;

#[derive(Debug, Clone)]
pub struct TimeManager {
  state: Arc<RwLock<TimeInfo>>,
  client_man: ClientMan,
}

#[derive(Debug, thiserror::Error)]
pub enum TimeError {
  #[error("broadcast failed for {0} client(s)")]
  Broadcast(usize),
}

impl TimeManager {
  pub fn new(client_man: ClientMan) -> Self {
    Self {
      state: Arc::new(RwLock::new(TimeInfo::default())),
      client_man,
    }
  }

  pub async fn snapshot(&self) -> TimeInfo {
    self.state.read().await.clone()
  }

  pub async fn apply_iap2_update(
    &self,
    seconds_since_reference_date: i64,
    tz_offset_minutes: i16,
    dst_offset_minutes: i8,
  ) -> Result<(), TimeError> {
    {
      let mut guard = self.state.write().await;
      guard.wall_clock_unix_s = u32::try_from(seconds_since_reference_date).ok();
      guard.utc_offset_minutes = Some(tz_offset_minutes);
      guard.dst_offset_minutes = Some(dst_offset_minutes);
    }
    self.broadcast().await
  }

  pub async fn apply_companion_snapshot(&self, info: TimeInfo) -> Result<(), TimeError> {
    {
      let mut guard = self.state.write().await;
      *guard = info;
    }
    self.broadcast().await
  }

  async fn broadcast(&self) -> Result<(), TimeError> {
    let snapshot = self.snapshot().await;
    let event = BridgeToClientTimeMsg::Changed(TimeSnapshot { time: snapshot });
    if let Err(errors) = self.client_man.broadcast(event, MsgMeta::Event).await {
      return Err(TimeError::Broadcast(errors.len()));
    }
    Ok(())
  }
}
