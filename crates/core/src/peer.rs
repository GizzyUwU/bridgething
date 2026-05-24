//! Live peer-state tracker. Owns the runtime view of every paired or
//! transient counterpart of the daemon (phone today, desktop later)
//! and is the single broadcast site for any change to that view.
//!
//! Three transports feed it: BlueZ pairing (sets `paired`), the iAP2
//! control session (sets `iap2`), and the bridgething gateway
//! protocol (sets `companion`). Each transport's manager sends a
//! command via [`PeerTracker`]'s channel; the actor task owns delta
//! detection and fires both the new `BridgeToClientPeerMsg` snapshot
//! and the legacy `BridgeToClientBluetoothMsg` /
//! `BridgeToClientAssetMsg::GatewayStatus` events derived from the
//! same transition. The legacy fires let the stock webapp keep
//! working without it understanding the new state model.
//!
//! `PairingResult{success=true}` is fired separately via
//! `confirm_pairing(mac)`, gated on a prior `note_pin_shown(mac)` from
//! the BlueZ agent. This decouples PIN-clearing from `paired`
//! transitions because BlueZ does not reliably toggle `Paired` during
//! re-pair on a cached device.

use std::collections::{HashMap, HashSet};

use bluer::Address;
use libbridgething::{
  Device, GatewayInfo, Peer, PeerCompanionStatus, PeerIap2Status,
  client::{
    BluetoothPairingResult, BluetoothStatus, BridgeToClientBluetoothMsg, BridgeToClientPeerMsg,
    ConnectedDevice as WireConnectedDevice, PairedDevicesMap, PeerSnapshotMap,
  },
  wire::MsgMeta,
};
use tokio::sync::{mpsc, watch};

use crate::{
  capabilities::CapabilitiesRegistry,
  net::{WSError, WireEventBus},
  player::Player,
  state::RouteTable,
  stock::{broadcast_stock_connection, broadcast_stock_disconnection},
};

const PEER_CMD_CAPACITY: usize = 64;

#[derive(Debug, Default, Clone)]
pub struct PeerSnapshot {
  pub peers: HashMap<Address, Peer>,
}

#[derive(Debug)]
enum PeerCommand {
  Upsert {
    mac: Address,
    device: Device,
  },
  EnsureExists {
    mac: Address,
    device: Device,
  },
  SetPaired {
    mac: Address,
    paired: bool,
  },
  SetIap2 {
    mac: Address,
    iap2: PeerIap2Status,
  },
  SetCompanion {
    mac: Address,
    companion: PeerCompanionStatus,
  },
  SetDisplayName {
    mac: Address,
    name: String,
  },
  SetLanguage {
    mac: Address,
    language: String,
  },
  SetUuid {
    mac: Address,
    uuid: String,
  },
  Remove {
    mac: Address,
  },
  NotePinShown {
    mac: Address,
  },
  ConfirmPairing {
    mac: Address,
  },
  ResyncStockConnection,
}

#[derive(Debug, Clone)]
pub struct PeerTracker {
  cmd_tx: mpsc::Sender<PeerCommand>,
  snapshot_rx: watch::Receiver<PeerSnapshot>,
}

impl PeerTracker {
  pub fn new(
    bus: WireEventBus,
    player: Player,
    capabilities: CapabilitiesRegistry,
    ws_routes: RouteTable,
    stream_routes: RouteTable,
  ) -> Self {
    let (cmd_tx, cmd_rx) = mpsc::channel(PEER_CMD_CAPACITY);
    let (snapshot_tx, snapshot_rx) = watch::channel(PeerSnapshot::default());
    tokio::spawn(run_actor(
      cmd_rx,
      snapshot_tx,
      bus,
      player,
      capabilities,
      ws_routes,
      stream_routes,
    ));
    Self { cmd_tx, snapshot_rx }
  }

  pub fn snapshot(&self) -> PeerSnapshot {
    self.snapshot_rx.borrow().clone()
  }

  pub fn first_connected_gateway(&self) -> Option<GatewayInfo> {
    self
      .snapshot_rx
      .borrow()
      .peers
      .values()
      .find_map(|peer| match &peer.companion {
        PeerCompanionStatus::Connected(info) => Some(info.clone()),
        _ => None,
      })
  }

  pub async fn upsert(&self, mac: Address, device: Device) {
    self.send(PeerCommand::Upsert { mac, device }).await;
  }

  pub async fn ensure_exists(&self, mac: Address, device: Device) {
    self.send(PeerCommand::EnsureExists { mac, device }).await;
  }

  pub async fn set_paired(&self, mac: Address, paired: bool) {
    self.send(PeerCommand::SetPaired { mac, paired }).await;
  }

  pub async fn set_iap2(&self, mac: Address, iap2: PeerIap2Status) {
    self.send(PeerCommand::SetIap2 { mac, iap2 }).await;
  }

  pub async fn set_companion(&self, mac: Address, companion: PeerCompanionStatus) {
    self.send(PeerCommand::SetCompanion { mac, companion }).await;
  }

  pub async fn set_display_name(&self, mac: Address, name: String) {
    self.send(PeerCommand::SetDisplayName { mac, name }).await;
  }

  pub async fn set_language(&self, mac: Address, language: String) {
    self.send(PeerCommand::SetLanguage { mac, language }).await;
  }

  pub async fn set_uuid(&self, mac: Address, uuid: String) {
    self.send(PeerCommand::SetUuid { mac, uuid }).await;
  }

  pub async fn remove(&self, mac: Address) {
    self.send(PeerCommand::Remove { mac }).await;
  }

  pub async fn note_pin_shown(&self, mac: Address) {
    self.send(PeerCommand::NotePinShown { mac }).await;
  }

  pub async fn confirm_pairing(&self, mac: Address) {
    self.send(PeerCommand::ConfirmPairing { mac }).await;
  }

  pub async fn resync_stock_connection(&self) {
    self.send(PeerCommand::ResyncStockConnection).await;
  }

  async fn send(&self, cmd: PeerCommand) {
    if self.cmd_tx.send(cmd).await.is_err() {
      tracing::warn!("peer tracker: command channel closed; command dropped");
    }
  }
}

struct PeerActor {
  peers: HashMap<Address, Peer>,
  pin_pending: HashSet<Address>,
  bus: WireEventBus,
  player: Player,
  capabilities: CapabilitiesRegistry,
  ws_routes: RouteTable,
  stream_routes: RouteTable,
  snapshot_tx: watch::Sender<PeerSnapshot>,
}

async fn run_actor(
  mut cmd_rx: mpsc::Receiver<PeerCommand>,
  snapshot_tx: watch::Sender<PeerSnapshot>,
  bus: WireEventBus,
  player: Player,
  capabilities: CapabilitiesRegistry,
  ws_routes: RouteTable,
  stream_routes: RouteTable,
) {
  let mut actor = PeerActor {
    peers: HashMap::new(),
    pin_pending: HashSet::new(),
    bus,
    player,
    capabilities,
    ws_routes,
    stream_routes,
    snapshot_tx,
  };

  while let Some(cmd) = cmd_rx.recv().await {
    actor.handle(cmd).await;
  }
  tracing::debug!("peer actor: command channel closed; exiting");
}

impl PeerActor {
  async fn handle(&mut self, cmd: PeerCommand) {
    match cmd {
      PeerCommand::Upsert { mac, device } => self.upsert(mac, device).await,
      PeerCommand::EnsureExists { mac, device } => self.ensure_exists(mac, device).await,
      PeerCommand::SetPaired { mac, paired } => self.set_paired(mac, paired).await,
      PeerCommand::SetIap2 { mac, iap2 } => self.set_iap2(mac, iap2).await,
      PeerCommand::SetCompanion { mac, companion } => self.set_companion(mac, companion).await,
      PeerCommand::SetDisplayName { mac, name } => self.set_display_name(mac, name).await,
      PeerCommand::SetLanguage { mac, language } => self.set_language(mac, language).await,
      PeerCommand::SetUuid { mac, uuid } => self.set_uuid(mac, uuid).await,
      PeerCommand::Remove { mac } => self.remove(mac).await,
      PeerCommand::NotePinShown { mac } => {
        self.pin_pending.insert(mac);
      }
      PeerCommand::ConfirmPairing { mac } => self.confirm_pairing(mac).await,
      PeerCommand::ResyncStockConnection => self.resync_stock_connection().await,
    }
  }

  fn publish_snapshot(&self) {
    let _ = self.snapshot_tx.send(PeerSnapshot {
      peers: self.peers.clone(),
    });
  }

  async fn upsert(&mut self, mac: Address, device: Device) {
    let prior = self.peers.get(&mac).cloned();
    let entry = self.peers.entry(mac).or_insert_with(|| Peer::new(device.clone()));
    entry.device = device;
    let diff = Diff::compute(mac, prior, Some(entry.clone()), &self.peers);
    self.broadcast_diff(diff).await;
  }

  async fn ensure_exists(&mut self, mac: Address, device: Device) {
    if self.peers.contains_key(&mac) {
      return;
    }
    let entry = Peer::new(device);
    self.peers.insert(mac, entry.clone());
    let diff = Diff::compute(mac, None, Some(entry), &self.peers);
    self.broadcast_diff(diff).await;
  }

  async fn set_paired(&mut self, mac: Address, paired: bool) {
    let Some(peer) = self.peers.get_mut(&mac) else {
      return;
    };
    let prior = peer.clone();
    peer.paired = paired;
    let snapshot = peer.clone();
    let diff = Diff::compute(mac, Some(prior), Some(snapshot), &self.peers);
    self.broadcast_diff(diff).await;
  }

  async fn set_iap2(&mut self, mac: Address, iap2: PeerIap2Status) {
    let Some(peer) = self.peers.get_mut(&mac) else {
      return;
    };
    let prior = peer.clone();
    peer.iap2 = iap2;
    let snapshot = peer.clone();
    let diff = Diff::compute(mac, Some(prior), Some(snapshot), &self.peers);
    self.broadcast_diff(diff).await;
  }

  async fn set_companion(&mut self, mac: Address, companion: PeerCompanionStatus) {
    let Some(peer) = self.peers.get_mut(&mac) else {
      return;
    };
    let prior = peer.clone();
    peer.companion = companion;
    let snapshot = peer.clone();
    let diff = Diff::compute(mac, Some(prior), Some(snapshot), &self.peers);
    self.broadcast_diff(diff).await;
  }

  async fn set_display_name(&mut self, mac: Address, display_name: String) {
    let Some(peer) = self.peers.get_mut(&mac) else {
      return;
    };
    let prior = peer.clone();
    peer.display_name = Some(display_name);
    let snapshot = peer.clone();
    let diff = Diff::compute(mac, Some(prior), Some(snapshot), &self.peers);
    self.broadcast_diff(diff).await;
  }

  async fn set_language(&mut self, mac: Address, language: String) {
    let Some(peer) = self.peers.get_mut(&mac) else {
      return;
    };
    let prior = peer.clone();
    peer.language = Some(language);
    let snapshot = peer.clone();
    let diff = Diff::compute(mac, Some(prior), Some(snapshot), &self.peers);
    self.broadcast_diff(diff).await;
  }

  async fn set_uuid(&mut self, mac: Address, uuid: String) {
    let Some(peer) = self.peers.get_mut(&mac) else {
      return;
    };
    let prior = peer.clone();
    peer.uuid = Some(uuid);
    let snapshot = peer.clone();
    let diff = Diff::compute(mac, Some(prior), Some(snapshot), &self.peers);
    self.broadcast_diff(diff).await;
  }

  async fn remove(&mut self, mac: Address) {
    self.pin_pending.remove(&mac);
    let prior = self.peers.remove(&mac);
    if prior.is_none() {
      return;
    }
    let diff = Diff::compute(mac, prior, None, &self.peers);
    self.broadcast_diff(diff).await;
  }

  async fn confirm_pairing(&mut self, mac: Address) {
    let was_pending = self.pin_pending.remove(&mac);
    if !was_pending {
      return;
    }
    if let Err(errs) = self
      .bus
      .broadcast(
        BridgeToClientBluetoothMsg::PairingResult(BluetoothPairingResult { success: true }),
        MsgMeta::Event,
      )
      .await
    {
      log_broadcast_errors("confirm_pairing", errs);
    }
  }

  async fn resync_stock_connection(&mut self) {
    let device = self
      .peers
      .values()
      .find(|p| p.has_useful_link())
      .map(|p| p.device.clone());
    let Some(device) = device else {
      return;
    };
    if let Err(errs) = broadcast_stock_connection(&self.bus, &device, &self.capabilities).await {
      log_broadcast_errors("resync_stock_connection", errs);
    }
    if let Err(err) = self.player.send_state().await {
      tracing::warn!(?err, "failed to send player state during stock resync");
    }
  }

  async fn broadcast_diff(&mut self, diff: Diff) {
    self.publish_snapshot();

    if let Err(errs) = self
      .bus
      .broadcast(
        BridgeToClientPeerMsg::Snapshot(PeerSnapshotMap(diff.snapshot.clone())),
        MsgMeta::Event,
      )
      .await
    {
      log_broadcast_errors("peer snapshot", errs);
    }

    if diff.paired_set_changed {
      let paired_map: HashMap<String, Device> = diff
        .snapshot
        .values()
        .filter(|p| p.paired)
        .map(|p| (p.device.mac.clone(), p.device.clone()))
        .collect();
      if let Err(errs) = self
        .bus
        .broadcast(
          BridgeToClientBluetoothMsg::PairedDevices(PairedDevicesMap(paired_map)),
          MsgMeta::Event,
        )
        .await
      {
        log_broadcast_errors("paired devices", errs);
      }
    }

    if diff.paired_transitioned_up {
      self.pin_pending.remove(&diff.mac);
      if let Err(errs) = self
        .bus
        .broadcast(
          BridgeToClientBluetoothMsg::PairingResult(BluetoothPairingResult { success: true }),
          MsgMeta::Event,
        )
        .await
      {
        log_broadcast_errors("pairing result", errs);
      }
    }

    if diff.useful_link_transitioned_up {
      if let Some(device) = diff.useful_device.as_ref() {
        if let Err(errs) = self
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
          log_broadcast_errors("connected device", errs);
        }
        if let Err(errs) = broadcast_stock_connection(&self.bus, device, &self.capabilities).await {
          log_broadcast_errors("stock connection", errs);
        }
        if let Err(err) = self.player.send_state().await {
          tracing::warn!(?err, "failed to send player state after useful link came up");
        }
      }
      if let Err(errs) = self
        .bus
        .broadcast(
          BridgeToClientBluetoothMsg::Status(BluetoothStatus { connected: true }),
          MsgMeta::Event,
        )
        .await
      {
        log_broadcast_errors("bluetooth status up", errs);
      }
    } else if diff.useful_link_transitioned_down {
      if let Err(errs) = self
        .bus
        .broadcast(
          BridgeToClientBluetoothMsg::Status(BluetoothStatus { connected: false }),
          MsgMeta::Event,
        )
        .await
      {
        log_broadcast_errors("bluetooth status down", errs);
      }
      if let Err(errs) = broadcast_stock_disconnection(&self.bus).await {
        log_broadcast_errors("stock disconnection", errs);
      }
    }

    if let Some(addr) = diff.companion_lost {
      if let Err(err) = self.capabilities.clear_companion(addr).await {
        tracing::warn!(?err, "failed to clear companion capabilities on disconnect");
      }
      self.tear_down_net_routes().await;
    }
  }

  async fn tear_down_net_routes(&self) {
    use libbridgething::{
      NetError, StreamError, WsError,
      client::{BridgeToClientNetMsgEvent, NetWsClosed, NetWsErrorEvent},
    };

    for (connection_id, owner) in self.ws_routes.drain_all() {
      let event = BridgeToClientNetMsgEvent::WsErrorEvent(NetWsErrorEvent {
        connection_id,
        error: WsError::GatewayDisconnected,
      });
      if let Err(err) = self.bus.send_event(owner, event).await {
        tracing::trace!(?err, "ws cleanup send failed");
      }
      let closed = BridgeToClientNetMsgEvent::WsClosed(NetWsClosed {
        connection_id,
        code: 1006,
        reason: "gateway disconnected".into(),
      });
      if let Err(err) = self.bus.send_event(owner, closed).await {
        tracing::trace!(?err, "ws cleanup send failed");
      }
    }

    for (stream_id, owner) in self.stream_routes.drain_all() {
      let event = BridgeToClientNetMsgEvent::StreamError(StreamError {
        stream_id,
        error: NetError::NoGateway,
      });
      if let Err(err) = self.bus.send_event(owner, event).await {
        tracing::trace!(?err, "stream cleanup send failed");
      }
    }
  }
}

fn log_broadcast_errors(label: &str, errs: Vec<WSError>) {
  tracing::debug!(count = errs.len(), "{label}: peer broadcast errors");
}

struct Diff {
  mac: Address,
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
      mac,
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
