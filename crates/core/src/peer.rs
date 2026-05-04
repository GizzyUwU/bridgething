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

use std::{collections::HashMap, sync::Arc};

use bluer::Address;
use libbridgething::{
  Device, GatewayInfo, Peer, PeerCompanionStatus, PeerIap2Status,
  client::{
    BluetoothPairingResult, BluetoothStatus, BridgeToClientBluetoothMsg, BridgeToClientPeerMsg,
    ConnectedDevice as WireConnectedDevice, PairedDevicesMap, PeerSnapshotMap,
  },
  wire::MsgMeta,
};
use tokio::sync::RwLock;

use crate::{
  capabilities::CapabilitiesRegistry,
  net::{WSError, WireEventBus},
  player::Player,
  state::RouteTable,
  stock::{broadcast_stock_connection, broadcast_stock_disconnection},
};

pub type PeerResult<T> = Result<T, Vec<WSError>>;

#[derive(Debug, Clone)]
pub struct PeerTracker {
  inner: Arc<Inner>,
}

#[derive(Debug)]
struct Inner {
  peers: RwLock<HashMap<Address, Peer>>,
  bus: WireEventBus,
  player: Player,
  capabilities: CapabilitiesRegistry,
  ws_routes: RouteTable,
  stream_routes: RouteTable,
}

impl PeerTracker {
  pub fn new(
    bus: WireEventBus,
    player: Player,
    capabilities: CapabilitiesRegistry,
    ws_routes: RouteTable,
    stream_routes: RouteTable,
  ) -> Self {
    Self {
      inner: Arc::new(Inner {
        peers: RwLock::new(HashMap::new()),
        bus,
        player,
        capabilities,
        ws_routes,
        stream_routes,
      }),
    }
  }

  pub async fn get(&self, mac: &Address) -> Option<Peer> {
    self.inner.peers.read().await.get(mac).cloned()
  }

  pub async fn first_connected_gateway(&self) -> Option<GatewayInfo> {
    let peers = self.inner.peers.read().await;
    peers.values().find_map(|peer| match &peer.companion {
      PeerCompanionStatus::Connected(info) => Some(info.clone()),
      _ => None,
    })
  }

  pub async fn upsert(&self, mac: Address, device: Device) -> PeerResult<()> {
    let diff = {
      let mut peers = self.inner.peers.write().await;
      let prior = peers.get(&mac).cloned();
      let entry = peers.entry(mac).or_insert_with(|| Peer::new(device.clone()));
      entry.device = device;
      Diff::compute(mac, prior, Some(entry.clone()), &peers)
    };
    self.broadcast_diff(diff).await
  }

  pub async fn set_paired(&self, mac: Address, paired: bool) -> PeerResult<()> {
    let diff = {
      let mut peers = self.inner.peers.write().await;
      let Some(peer) = peers.get_mut(&mac) else {
        return Ok(());
      };
      let prior = peer.clone();
      peer.paired = paired;
      Diff::compute(mac, Some(prior), Some(peer.clone()), &peers)
    };
    self.broadcast_diff(diff).await
  }

  pub async fn set_iap2(&self, mac: Address, iap2: PeerIap2Status) -> PeerResult<()> {
    let diff = {
      let mut peers = self.inner.peers.write().await;
      let Some(peer) = peers.get_mut(&mac) else {
        return Ok(());
      };
      let prior = peer.clone();
      peer.iap2 = iap2;
      Diff::compute(mac, Some(prior), Some(peer.clone()), &peers)
    };
    self.broadcast_diff(diff).await
  }

  pub async fn set_companion(&self, mac: Address, companion: PeerCompanionStatus) -> PeerResult<()> {
    let diff = {
      let mut peers = self.inner.peers.write().await;
      let Some(peer) = peers.get_mut(&mac) else {
        return Ok(());
      };
      let prior = peer.clone();
      peer.companion = companion;
      Diff::compute(mac, Some(prior), Some(peer.clone()), &peers)
    };
    self.broadcast_diff(diff).await
  }

  pub async fn set_display_name(&self, mac: Address, display_name: String) -> PeerResult<()> {
    let diff = {
      let mut peers = self.inner.peers.write().await;
      let Some(peer) = peers.get_mut(&mac) else {
        return Ok(());
      };
      let prior = peer.clone();
      peer.display_name = Some(display_name);
      Diff::compute(mac, Some(prior), Some(peer.clone()), &peers)
    };
    self.broadcast_diff(diff).await
  }

  pub async fn set_language(&self, mac: Address, language: String) -> PeerResult<()> {
    let diff = {
      let mut peers = self.inner.peers.write().await;
      let Some(peer) = peers.get_mut(&mac) else {
        return Ok(());
      };
      let prior = peer.clone();
      peer.language = Some(language);
      Diff::compute(mac, Some(prior), Some(peer.clone()), &peers)
    };
    self.broadcast_diff(diff).await
  }

  pub async fn set_uuid(&self, mac: Address, uuid: String) -> PeerResult<()> {
    let diff = {
      let mut peers = self.inner.peers.write().await;
      let Some(peer) = peers.get_mut(&mac) else {
        return Ok(());
      };
      let prior = peer.clone();
      peer.uuid = Some(uuid);
      Diff::compute(mac, Some(prior), Some(peer.clone()), &peers)
    };
    self.broadcast_diff(diff).await
  }

  pub async fn remove(&self, mac: Address) -> PeerResult<()> {
    let diff = {
      let mut peers = self.inner.peers.write().await;
      let prior = peers.remove(&mac);
      if prior.is_none() {
        return Ok(());
      }
      Diff::compute(mac, prior, None, &peers)
    };
    self.broadcast_diff(diff).await
  }

  async fn broadcast_diff(&self, diff: Diff) -> PeerResult<()> {
    let mut errors: Vec<WSError> = Vec::new();

    if let Err(errs) = self
      .inner
      .bus
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
        .inner
        .bus
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
        .inner
        .bus
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
          .inner
          .bus
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
        if let Err(errs) = broadcast_stock_connection(&self.inner.bus, device, &self.inner.capabilities).await {
          errors.extend(errs);
        }
        if let Err(err) = self.inner.player.send_state().await {
          tracing::warn!(?err, "failed to send player state after useful link came up");
        }
      }
      if let Err(errs) = self
        .inner
        .bus
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
        .inner
        .bus
        .broadcast(
          BridgeToClientBluetoothMsg::Status(BluetoothStatus { connected: false }),
          MsgMeta::Event,
        )
        .await
      {
        errors.extend(errs);
      }
      if let Err(errs) = broadcast_stock_disconnection(&self.inner.bus).await {
        errors.extend(errs);
      }
    }

    if let Some(addr) = diff.companion_lost {
      if let Err(err) = self.inner.capabilities.clear_companion(addr).await {
        tracing::warn!(?err, "failed to clear companion capabilities on disconnect");
      }
      self.tear_down_net_routes().await;
    }

    if errors.is_empty() { Ok(()) } else { Err(errors) }
  }

  async fn tear_down_net_routes(&self) {
    use libbridgething::{
      NetError, StreamError, WsError,
      client::{BridgeToClientNetMsgEvent, NetWsClosed, NetWsErrorEvent},
    };

    for (connection_id, owner) in self.inner.ws_routes.drain_all() {
      let event = BridgeToClientNetMsgEvent::WsErrorEvent(NetWsErrorEvent {
        connection_id,
        error: WsError::GatewayDisconnected,
      });
      if let Err(err) = self.inner.bus.send_event(owner, event).await {
        tracing::trace!(?err, "ws cleanup send failed");
      }
      let closed = BridgeToClientNetMsgEvent::WsClosed(NetWsClosed {
        connection_id,
        code: 1006,
        reason: "gateway disconnected".into(),
      });
      if let Err(err) = self.inner.bus.send_event(owner, closed).await {
        tracing::trace!(?err, "ws cleanup send failed");
      }
    }

    for (stream_id, owner) in self.inner.stream_routes.drain_all() {
      let event = BridgeToClientNetMsgEvent::StreamError(StreamError {
        stream_id,
        error: NetError::NoGateway,
      });
      if let Err(err) = self.inner.bus.send_event(owner, event).await {
        tracing::trace!(?err, "stream cleanup send failed");
      }
    }
  }

  pub async fn resync_stock_connection(&self) -> PeerResult<()> {
    let device = {
      let peers = self.inner.peers.read().await;
      peers.values().find(|p| p.has_useful_link()).map(|p| p.device.clone())
    };
    let Some(device) = device else {
      return Ok(());
    };
    broadcast_stock_connection(&self.inner.bus, &device, &self.inner.capabilities).await?;
    if let Err(err) = self.inner.player.send_state().await {
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
  companion_lost: Option<Address>,
}

impl Diff {
  fn compute(mac: Address, prior: Option<Peer>, current: Option<Peer>, peers: &HashMap<Address, Peer>) -> Self {
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
    let companion_lost = if was_companion_connected && !is_companion_connected {
      Some(mac)
    } else {
      None
    };

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
