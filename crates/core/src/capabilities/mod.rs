use std::{
  collections::HashMap,
  net::SocketAddr,
  sync::{Arc, RwLock},
};

use bluer::Address;
use libbridgething::{
  Capabilities, CompanionAuthorityScope, GatewayCapabilities, SurfaceAvailability,
  client::{BridgeToClientCapabilitiesMsgEvent, CapabilitiesSnapshot},
};

use crate::{
  authority::AuthorityRegistry,
  net::{WSResult, WireEventBus},
};

#[derive(Debug, Clone)]
pub struct CapabilitiesRegistry {
  inner: Arc<Inner>,
}

#[derive(Debug)]
struct Inner {
  snapshot: RwLock<Capabilities>,
  announces: RwLock<HashMap<Address, GatewayCapabilities>>,
  bus: WireEventBus,
  authority: AuthorityRegistry,
}

impl CapabilitiesRegistry {
  pub fn new(bus: WireEventBus, authority: AuthorityRegistry) -> Self {
    Self {
      inner: Arc::new(Inner {
        snapshot: RwLock::new(Capabilities::default()),
        announces: RwLock::new(HashMap::new()),
        bus,
        authority,
      }),
    }
  }

  pub fn snapshot(&self) -> Capabilities {
    self.inner.snapshot.read().expect("capabilities lock poisoned").clone()
  }

  pub async fn set_announce(&self, addr: Address, mut caps: GatewayCapabilities) -> WSResult<()> {
    caps.uri_schemes = normalize_schemes(caps.uri_schemes);
    {
      let mut guard = self.inner.announces.write().expect("announces lock poisoned");
      guard.insert(addr, caps);
    }
    self.rebuild_and_broadcast().await
  }

  pub async fn clear_companion(&self, addr: Address) -> WSResult<()> {
    {
      let mut guard = self.inner.announces.write().expect("announces lock poisoned");
      guard.remove(&addr);
    }
    self.inner.authority.drop_all();
    self.rebuild_and_broadcast().await
  }

  pub async fn claim_authority(&self, scope: CompanionAuthorityScope, app_bundle: Option<String>) -> WSResult<()> {
    self.inner.authority.claim(scope);
    self.inner.authority.set_companion_app_bundle(app_bundle);
    self.rebuild_and_broadcast().await
  }

  pub async fn release_authority(&self, scope: CompanionAuthorityScope) -> WSResult<()> {
    self.inner.authority.release(scope);
    self.rebuild_and_broadcast().await
  }

  pub async fn send_snapshot_to(&self, to: SocketAddr) -> WSResult<()> {
    let caps = self.snapshot();
    let event = BridgeToClientCapabilitiesMsgEvent::Update(CapabilitiesSnapshot { capabilities: caps });
    self.inner.bus.send_event(to, event).await
  }

  fn build_snapshot(&self) -> Capabilities {
    let announces = self.inner.announces.read().expect("announces lock poisoned");
    let primary = announces.values().next();
    let authority = self.inner.authority.live_scopes();

    match primary {
      Some(caps) => Capabilities {
        gateway: Some(caps.gateway.clone()),
        available: caps.available,
        authority,
        uri_schemes: caps.uri_schemes.clone(),
        network: caps.network,
        audio: caps.audio.clone(),
        music_provider: caps.music_provider,
      },
      None => Capabilities {
        gateway: None,
        available: SurfaceAvailability::default(),
        authority,
        uri_schemes: Vec::new(),
        network: Default::default(),
        audio: Default::default(),
        music_provider: Default::default(),
      },
    }
  }

  async fn rebuild_and_broadcast(&self) -> WSResult<()> {
    let snapshot = self.build_snapshot();
    {
      let mut guard = self.inner.snapshot.write().expect("capabilities lock poisoned");
      *guard = snapshot.clone();
    }
    let event = BridgeToClientCapabilitiesMsgEvent::Update(CapabilitiesSnapshot { capabilities: snapshot });
    match self.inner.bus.broadcast_event(event).await {
      Ok(()) => Ok(()),
      Err(errs) => {
        for err in &errs {
          tracing::warn!(?err, "capabilities broadcast partial failure");
        }

        match errs.into_iter().next() {
          Some(e) => Err(e),
          None => Ok(()),
        }
      }
    }
  }
}

fn normalize_schemes(schemes: Vec<String>) -> Vec<String> {
  let mut seen: Vec<String> = Vec::with_capacity(schemes.len());
  for raw in schemes {
    let trimmed = raw.trim().trim_end_matches(':').to_ascii_lowercase();
    if !is_valid_scheme(&trimmed) {
      continue;
    }
    if !seen.iter().any(|s| s == &trimmed) {
      seen.push(trimmed);
    }
  }
  seen
}

fn is_valid_scheme(s: &str) -> bool {
  let mut chars = s.chars();
  let Some(first) = chars.next() else { return false };
  if !first.is_ascii_alphabetic() {
    return false;
  }
  chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
}

#[cfg(test)]
mod tests {
  use libbridgething::{AudioCapabilities, GatewayInfo, MusicProvider, NetworkInfo, NetworkKind, SurfaceAvailability};

  use super::*;

  fn caps_with(schemes: Vec<&str>, available: SurfaceAvailability) -> GatewayCapabilities {
    GatewayCapabilities {
      gateway: GatewayInfo {
        address: "00:11:22:33:44:55".into(),
        name: "test".into(),
        os_name: "ios".into(),
        ..Default::default()
      },
      uri_schemes: schemes.into_iter().map(String::from).collect(),
      network: NetworkInfo {
        kind: NetworkKind::Wifi,
        metered: false,
      },
      available,
      audio: AudioCapabilities::default(),
      music_provider: MusicProvider::Spotify,
    }
  }

  #[test]
  fn normalizes_schemes() {
    let out = normalize_schemes(vec![
      "Spotify:".into(),
      "  apple-music: ".into(),
      "spotify".into(), // dedup
      "".into(),
      "1bad".into(), // invalid
      "ok+v.2".into(),
    ]);
    assert_eq!(out, vec!["spotify", "apple-music", "ok+v.2"]);
  }

  #[tokio::test]
  async fn snapshot_empty_without_announce() {
    let (client_man, _listener) = crate::net::create_client_manager();
    let bus = WireEventBus::new(client_man);
    let auth = AuthorityRegistry::new();
    let reg = CapabilitiesRegistry::new(bus, auth);
    let snap = reg.snapshot();
    assert!(snap.gateway.is_none());
    assert!(snap.uri_schemes.is_empty());
    assert!(snap.authority.is_empty());
    assert_eq!(snap.available, SurfaceAvailability::default());
  }

  #[tokio::test]
  async fn announce_populates_snapshot_with_companion_availability() {
    let (client_man, _listener) = crate::net::create_client_manager();
    let bus = WireEventBus::new(client_man);
    let auth = AuthorityRegistry::new();
    let reg = CapabilitiesRegistry::new(bus, auth);

    let addr: Address = "00:11:22:33:44:55".parse().unwrap();
    let claimed = SurfaceAvailability {
      geo: true,
      notifications: true,
      net_fetch: true,
      net_ws: true,
      audio_tts: true,
      lyrics: true,
    };
    let caps = caps_with(vec!["spotify:", "Apple-Music"], claimed);
    let _ = reg.set_announce(addr, caps).await;

    let snap = reg.snapshot();
    assert!(snap.gateway.is_some());
    assert_eq!(snap.uri_schemes, vec!["spotify", "apple-music"]);
    assert_eq!(snap.available, claimed);
  }

  #[tokio::test]
  async fn authority_mutations_appear_in_snapshot() {
    let (client_man, _listener) = crate::net::create_client_manager();
    let bus = WireEventBus::new(client_man);
    let auth = AuthorityRegistry::new();
    let reg = CapabilitiesRegistry::new(bus, auth.clone());

    let _ = reg
      .claim_authority(CompanionAuthorityScope::NowPlayingMetadata, None)
      .await;
    let snap = reg.snapshot();
    assert_eq!(snap.authority, vec![CompanionAuthorityScope::NowPlayingMetadata]);
    assert!(auth.is_authoritative(CompanionAuthorityScope::NowPlayingMetadata));

    let _ = reg.release_authority(CompanionAuthorityScope::NowPlayingMetadata).await;
    assert!(reg.snapshot().authority.is_empty());
  }

  #[tokio::test]
  async fn clear_companion_drops_announce_and_authority() {
    let (client_man, _listener) = crate::net::create_client_manager();
    let bus = WireEventBus::new(client_man);
    let auth = AuthorityRegistry::new();
    let reg = CapabilitiesRegistry::new(bus, auth.clone());

    let addr: Address = "00:11:22:33:44:55".parse().unwrap();
    let _ = reg
      .set_announce(addr, caps_with(vec!["spotify"], SurfaceAvailability::default()))
      .await;
    let _ = reg
      .claim_authority(CompanionAuthorityScope::NowPlayingPlayback, None)
      .await;
    assert!(reg.snapshot().gateway.is_some());
    assert!(!reg.snapshot().authority.is_empty());

    let _ = reg.clear_companion(addr).await;
    let snap = reg.snapshot();
    assert!(snap.gateway.is_none());
    assert!(snap.uri_schemes.is_empty());
    assert!(snap.authority.is_empty());
    assert!(!auth.is_authoritative(CompanionAuthorityScope::NowPlayingPlayback));
  }
}
