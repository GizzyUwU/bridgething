//! Live peer-state tracker. Owns the runtime view of every paired or
//! transient counterpart of the daemon (phone today, desktop later)
//! and is the single broadcast site for any change to that view.
//!
//! Three transports feed it: BlueZ pairing (sets `paired`), the iAP2
//! control session (sets `iap2`), and the bridgething gateway
//! protocol (sets `companion`). Each transport's manager calls the
//! matching `set_*` method; this module owns delta detection and
//! fires both the new `BridgeToClientPeerMsg` snapshot and the legacy
//! `BridgeToClientBluetoothMsg` / `BridgeToClientAssetMsg::GatewayStatus`
//! events derived from the same transition. The legacy fires let the
//! stock webapp keep working without it understanding the new state
//! model.
//!
//! Internally a `RwLock<HashMap>` for now. The migration path to a
//! dedicated mpsc-driven actor is mechanical and intentional - move
//! the map behind a command channel, every public method becomes a
//! oneshot send/await, no caller-side change.

use std::collections::HashMap;

use bluer::Address;
use libbridgething::{
  Device, Peer, PeerCompanionStatus, PeerIap2Status,
  client::{
    BluetoothPairingResult, BluetoothStatus, BridgeToClientBluetoothMsg, BridgeToClientPeerMsg,
    ConnectedDevice as WireConnectedDevice, GatewayStatus, PairedDevicesMap, PeerSnapshotMap,
  },
  wire::MsgMeta,
};
use tokio::sync::RwLock;

use crate::{
  authority::AuthorityRegistry,
  net::{ClientMan, WSError},
  player::Player,
  stock::{broadcast_stock_connection, broadcast_stock_disconnection},
};

pub type PeerResult<T> = Result<T, Vec<WSError>>;

#[derive(Debug)]
pub struct PeerTracker {
  inner: RwLock<HashMap<Address, Peer>>,
  client_man: ClientMan,
  player: Player,
  authority: AuthorityRegistry,
}

impl PeerTracker {
  pub fn new(client_man: ClientMan, player: Player, authority: AuthorityRegistry) -> Self {
    Self {
      inner: RwLock::new(HashMap::new()),
      client_man,
      player,
      authority,
    }
  }

  pub async fn get(&self, mac: &Address) -> Option<Peer> {
    self.inner.read().await.get(mac).cloned()
  }

  pub async fn first_connected_gateway(&self) -> GatewayStatus {
    let peers = self.inner.read().await;
    for peer in peers.values() {
      if let PeerCompanionStatus::Connected(meta) = &peer.companion {
        return GatewayStatus {
          address: peer.device.mac.clone(),
          connected: true,
          adapter_version: meta.adapter_version.clone(),
          lib_version: meta.lib_version.clone(),
          libbridgething_version: meta.libbridgething_version.clone(),
          app_name: meta.app_name.clone(),
          app_version: meta.app_version.clone(),
          os_name: meta.os_name.clone(),
        };
      }
    }
    GatewayStatus::default()
  }

  pub async fn upsert(&self, mac: Address, device: Device) -> PeerResult<()> {
    let diff = {
      let mut peers = self.inner.write().await;
      let prior = peers.get(&mac).cloned();
      let entry = peers.entry(mac).or_insert_with(|| Peer::new(device.clone()));
      entry.device = device;
      Diff::compute(prior, Some(entry.clone()), &peers)
    };
    self.broadcast_diff(diff).await
  }

  pub async fn set_paired(&self, mac: Address, paired: bool) -> PeerResult<()> {
    let diff = {
      let mut peers = self.inner.write().await;
      let Some(peer) = peers.get_mut(&mac) else {
        return Ok(());
      };
      let prior = peer.clone();
      peer.paired = paired;
      Diff::compute(Some(prior), Some(peer.clone()), &peers)
    };
    self.broadcast_diff(diff).await
  }

  pub async fn set_iap2(&self, mac: Address, iap2: PeerIap2Status) -> PeerResult<()> {
    let diff = {
      let mut peers = self.inner.write().await;
      let Some(peer) = peers.get_mut(&mac) else {
        return Ok(());
      };
      let prior = peer.clone();
      peer.iap2 = iap2;
      Diff::compute(Some(prior), Some(peer.clone()), &peers)
    };
    self.broadcast_diff(diff).await
  }

  pub async fn set_companion(&self, mac: Address, companion: PeerCompanionStatus) -> PeerResult<()> {
    let diff = {
      let mut peers = self.inner.write().await;
      let Some(peer) = peers.get_mut(&mac) else {
        return Ok(());
      };
      let prior = peer.clone();
      peer.companion = companion;
      Diff::compute(Some(prior), Some(peer.clone()), &peers)
    };
    self.broadcast_diff(diff).await
  }

  pub async fn remove(&self, mac: Address) -> PeerResult<()> {
    let diff = {
      let mut peers = self.inner.write().await;
      let prior = peers.remove(&mac);
      if prior.is_none() {
        return Ok(());
      }
      Diff::compute(prior, None, &peers)
    };
    self.broadcast_diff(diff).await
  }

  async fn broadcast_diff(&self, diff: Diff) -> PeerResult<()> {
    let mut errors: Vec<WSError> = Vec::new();

    if let Err(errs) = self
      .client_man
      .broadcast(
        BridgeToClientPeerMsg::Snapshot(PeerSnapshotMap(diff.snapshot.clone())),
        MsgMeta::Event,
      )
      .await
    {
      errors.extend(errs);
    }

    if diff.paired_set_changed {
      let paired_map: HashMap<String, Device> = diff
        .snapshot
        .values()
        .filter(|p| p.paired)
        .map(|p| (p.device.mac.clone(), p.device.clone()))
        .collect();
      if let Err(errs) = self
        .client_man
        .broadcast(
          BridgeToClientBluetoothMsg::PairedDevices(PairedDevicesMap(paired_map)),
          MsgMeta::Event,
        )
        .await
      {
        errors.extend(errs);
      }
    }

    if diff.paired_transitioned_up
      && let Err(errs) = self
        .client_man
        .broadcast(
          BridgeToClientBluetoothMsg::PairingResult(BluetoothPairingResult { success: true }),
          MsgMeta::Event,
        )
        .await
    {
      errors.extend(errs);
    }

    if diff.useful_link_transitioned_up {
      if let Some(device) = diff.useful_device.as_ref() {
        if let Err(errs) = self
          .client_man
          .broadcast(
            BridgeToClientBluetoothMsg::ConnectedDevice(WireConnectedDevice {
              name: device.name.clone(),
              mac: device.mac.clone(),
            }),
            MsgMeta::Event,
          )
          .await
        {
          errors.extend(errs);
        }
        if let Err(errs) = broadcast_stock_connection(&self.client_man, device).await {
          errors.extend(errs);
        }
        if let Err(err) = self.player.send_state().await {
          tracing::warn!(?err, "failed to send player state after useful link came up");
        }
      }
      if let Err(errs) = self
        .client_man
        .broadcast(
          BridgeToClientBluetoothMsg::Status(BluetoothStatus { connected: true }),
          MsgMeta::Event,
        )
        .await
      {
        errors.extend(errs);
      }
    } else if diff.useful_link_transitioned_down {
      if let Err(errs) = self
        .client_man
        .broadcast(
          BridgeToClientBluetoothMsg::Status(BluetoothStatus { connected: false }),
          MsgMeta::Event,
        )
        .await
      {
        errors.extend(errs);
      }
      if let Err(errs) = broadcast_stock_disconnection(&self.client_man).await {
        errors.extend(errs);
      }
    }

    if diff.companion_lost {
      self.authority.drop_all();
    }

    if errors.is_empty() { Ok(()) } else { Err(errors) }
  }

  /// Replays the stock connection broadcasts for the currently-useful peer,
  /// if any. Used when a stock webapp connects fresh and needs to be told
  /// the phone is already there - the regular useful_link transitions
  /// happened before this webapp opened its socket.
  pub async fn resync_stock_connection(&self) -> PeerResult<()> {
    let device = {
      let peers = self.inner.read().await;
      peers.values().find(|p| p.has_useful_link()).map(|p| p.device.clone())
    };
    let Some(device) = device else {
      return Ok(());
    };
    broadcast_stock_connection(&self.client_man, &device).await?;
    if let Err(err) = self.player.send_state().await {
      tracing::warn!(?err, "failed to send player state during stock resync");
    }
    Ok(())
  }
}

struct Diff {
  snapshot: HashMap<String, Peer>,
  paired_transitioned_up: bool,
  paired_set_changed: bool,
  useful_link_transitioned_up: bool,
  useful_link_transitioned_down: bool,
  useful_device: Option<Device>,
  companion_lost: bool,
}

impl Diff {
  fn compute(prior: Option<Peer>, current: Option<Peer>, peers: &HashMap<Address, Peer>) -> Self {
    let identity_mac = current.as_ref().or(prior.as_ref()).map(|p| p.device.mac.clone());

    let was_paired = prior.as_ref().is_some_and(|p| p.paired);
    let is_paired = current.as_ref().is_some_and(|p| p.paired);
    let was_useful_self = prior.as_ref().is_some_and(|p| p.has_useful_link());
    let is_useful_self = current.as_ref().is_some_and(|p| p.has_useful_link());

    let other_useful = peers
      .values()
      .filter(|p| identity_mac.as_deref() != Some(&p.device.mac))
      .any(|p| p.has_useful_link());
    let any_useful_before = other_useful || was_useful_self;
    let any_useful_now = other_useful || is_useful_self;

    let was_companion_connected = matches!(
      prior.as_ref().map(|p| &p.companion),
      Some(PeerCompanionStatus::Connected(_))
    );
    let is_companion_connected = matches!(
      current.as_ref().map(|p| &p.companion),
      Some(PeerCompanionStatus::Connected(_))
    );
    let companion_lost = was_companion_connected && !is_companion_connected;

    let snapshot = peers
      .iter()
      .map(|(addr, peer)| (addr.to_string(), peer.clone()))
      .collect();

    Self {
      snapshot,
      paired_transitioned_up: !was_paired && is_paired,
      paired_set_changed: was_paired != is_paired,
      useful_link_transitioned_up: !any_useful_before && any_useful_now,
      useful_link_transitioned_down: any_useful_before && !any_useful_now,
      useful_device: if !was_useful_self && is_useful_self {
        current.as_ref().map(|p| p.device.clone())
      } else {
        None
      },
      companion_lost,
    }
  }
}
