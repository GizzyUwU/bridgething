//! Seam scenarios: each body is written once over the Driver/Observer
//! capability traits and lifted to every tier that satisfies its bounds.
//! The per-tier wrappers below are the whole point - one scenario, one
//! assertion surface (the frame-tap, available everywhere), run at rising
//! fidelity. A lower tier passing where a higher tier passes is how we know
//! the lower tier is honest.
//!
//! AppState-asserting scenarios stay tier-local (android.rs / iap2.rs):
//! AppState only exists in-process, so those do not lift. The bodies here
//! assert on the egress frame stream, which every tier exposes.
//!
//! T3 wrappers are `#[ignore]` (need a booted Car Thing + a host BT radio):
//!   SUPERBIRD_BT_MAC=<mac> cargo test -p bridgething-test-harness --test seam -- --ignored --nocapture

use std::time::Duration;

use bridgething::{ClientMode, Iap2TransportCommand};
use bridgething_iap2::{
  HidCommand,
  csm::now_playing::{MediaItemAttributes as Iap2MediaItem, NowPlayingUpdate as Iap2NowPlaying},
};
use bridgething_test_harness::{
  CommandDriver, DeviceHarness, DeviceTier, FrameObserve, FrameObserver, GatewayDriver, Harness, Iap2OutboundObserve,
  Iap2Source, Iap2SourceDriver, ModernClientDriver, OverAirTransport,
};
use libbridgething::{
  CompanionAuthorityScope, GatewayCapabilities, GatewayInfo, GeoAccuracy, MediaItemUpdate, NowPlayingUpdate, PhoneCall,
  PhoneCallDirection, PhoneCallStatus, Position,
  client::{GeoWatch, SetShuffle},
  gateway::AuthorityClaim,
};

/// Time for a freshly connected client's capabilities snapshot to egress.
const SNAPSHOT_BARRIER: Duration = Duration::from_secs(10);

/// Time for a driven now-playing to converge onto the egress stream. Generous
/// so the slow over-air tiers (real auth + link timing) have room; the fast
/// in-process tier returns as soon as the frame matches.
const NOW_PLAYING_WAIT: Duration = Duration::from_secs(20);

/// Lift a scenario body (generic over the capability traits) to a set of
/// tiers, emitting one `#[tokio::test]` per tier inside a module named for the
/// scenario, so the case path reads `scenario::t1`, `scenario::t3_rfcomm`, ...
/// The compiler checks each listed tier satisfies the scenario's bounds, so an
/// incapable tier is a build error, not a silent skip. T3 tiers are `#[ignore]`
/// (they need a booted Car Thing + a host BT radio).
macro_rules! lift {
  ($scenario:ident, [$($tier:ident),+ $(,)?]) => {
    mod $scenario {
      $( lift!(@tier $tier, $scenario); )+
    }
  };
  (@tier t1, $scenario:ident) => {
    #[tokio::test]
    async fn t1() {
      let tier = super::Harness::start().await.expect("harness start");
      super::$scenario(&tier).await.expect(concat!(stringify!($scenario), " (t1)"));
    }
  };
  (@tier t3_rfcomm, $scenario:ident) => {
    #[tokio::test]
    #[ignore = "requires a booted Car Thing with the test-tap daemon + a host BT radio"]
    async fn t3_rfcomm() {
      let tier = super::DeviceTier::new(
        super::DeviceHarness::from_env().expect("device env (SUPERBIRD_BT_MAC)"),
        super::OverAirTransport::Rfcomm,
      );
      super::$scenario(&tier).await.expect(concat!(stringify!($scenario), " (t3 rfcomm)"));
    }
  };
  (@tier t3_iap2_ea, $scenario:ident) => {
    #[tokio::test]
    #[ignore = "requires a booted Car Thing with the test-tap daemon + a host BT radio"]
    async fn t3_iap2_ea() {
      let tier = super::DeviceTier::new(
        super::DeviceHarness::from_env().expect("device env (SUPERBIRD_BT_MAC)"),
        super::OverAirTransport::Iap2Ea,
      );
      super::$scenario(&tier).await.expect(concat!(stringify!($scenario), " (t3 iap2-ea)"));
    }
  };
  (@tier t3_emulator, $scenario:ident) => {
    #[tokio::test]
    #[ignore = "requires a booted Car Thing with the test-tap daemon + a host BT radio"]
    async fn t3_emulator() {
      let tier = super::DeviceHarness::from_env().expect("device env (SUPERBIRD_BT_MAC)");
      super::$scenario(&tier).await.expect(concat!(stringify!($scenario), " (t3 emulator)"));
    }
  };
}

fn caps() -> GatewayCapabilities {
  GatewayCapabilities {
    gateway: GatewayInfo {
      address: String::new(),
      name: "seam".into(),
      os_name: "android".into(),
      app_name: "seam".into(),
      app_version: "0.0.0".into(),
      adapter_version: "harness".into(),
      lib_version: "0.0.0".into(),
      libbridgething_version: format!("v{}", libbridgething::LIBBRIDGETHING_VERSION),
    },
    ..Default::default()
  }
}

/// Subscribe to the egress stream, connect a modern client as the broadcast
/// recipient, and wait for its capabilities snapshot to egress - proving it is
/// registered before the scenario drives anything. The client is drained in
/// the background so its WS pings keep answering through the seconds-long
/// over-air bring-up, or its channel would close before any frame egresses.
/// Frame-tap only, so this barrier is uniform across tiers (no AppState read).
async fn observe_with_registered_client<T>(tier: &T) -> anyhow::Result<FrameObserver>
where
  T: FrameObserve + ModernClientDriver,
{
  let mut frames = tier.frames().await?;
  let mut client = tier.modern_client().await?;
  tokio::spawn(async move { while client.recv().await.is_some() {} });
  let registered = frames
    .wait_for(SNAPSHOT_BARRIER, |f| f.mode == ClientMode::Modern)
    .await;
  anyhow::ensure!(registered.is_some(), "modern client snapshot never egressed");
  Ok(frames)
}

/// A gateway companion announces, claims metadata authority, and pushes a
/// track; the merged now-playing must reach the egress stream. Lifts across
/// T1 (in-process duplex) and both T3 over-air transports (rfcomm, iAP2 EA),
/// since the same `Gateway` rides all three.
async fn gateway_now_playing_reaches_frame_tap<T>(tier: &T) -> anyhow::Result<()>
where
  T: GatewayDriver + FrameObserve + ModernClientDriver,
{
  let mut frames = observe_with_registered_client(tier).await?;

  let gateway = tier.gateway().await?;
  gateway.capabilities().announce(caps()).await.expect("announce");
  gateway
    .authority()
    .claim(AuthorityClaim {
      scope: CompanionAuthorityScope::NowPlayingMetadata,
    })
    .await
    .expect("claim metadata authority");
  gateway
    .player()
    .delta(NowPlayingUpdate {
      media_item: Some(MediaItemUpdate {
        persistent_id: Some("seam-gateway".into()),
        title: Some("Seam Gateway Track".into()),
        artist: Some("Seam".into()),
        ..Default::default()
      }),
      playback: None,
    })
    .await
    .expect("push now-playing");

  let observed = frames
    .wait_for(NOW_PLAYING_WAIT, |f| f.json().contains("Seam Gateway Track"))
    .await;
  anyhow::ensure!(observed.is_some(), "gateway now-playing never reached the frame-tap");
  Ok(())
}

/// A single iAP2 source (no companion) pushes a now-playing delta; iAP2 is the
/// fallthrough source, so it merges without an authority claim and must reach
/// the egress stream. Lifts across T1 (event-inject) and T3 (the device-half
/// emulator's control session over the real radio).
async fn iap2_source_now_playing_reaches_frame_tap<T>(tier: &T) -> anyhow::Result<()>
where
  T: Iap2SourceDriver + FrameObserve + ModernClientDriver,
{
  let mut frames = observe_with_registered_client(tier).await?;

  let source = tier.iap2_source().await?;
  source
    .push_now_playing(Iap2NowPlaying {
      media_item: Some(Iap2MediaItem {
        persistent_id: Some(0x5EA3),
        title: Some("Seam iAP2 Track".into()),
        artist: Some("Seam".into()),
        ..Default::default()
      }),
      playback: None,
    })
    .await?;

  let observed = frames
    .wait_for(NOW_PLAYING_WAIT, |f| f.json().contains("Seam iAP2 Track"))
    .await;
  anyhow::ensure!(
    observed.is_some(),
    "iAP2 source now-playing never reached the frame-tap"
  );
  Ok(())
}

/// The production cover-art case, single iAP2 source. iOS announces artwork on a
/// track, then sends a same-track metadata delta (as the live progress stream
/// does) before the artwork bytes arrive. The resolved art id must reach the
/// egress stream - the intervening delta must not cost the artwork.
async fn single_source_artwork_reaches_frame_tap<T>(tier: &T) -> anyhow::Result<()>
where
  T: Iap2SourceDriver + FrameObserve + ModernClientDriver,
{
  let mut frames = observe_with_registered_client(tier).await?;
  let source = tier.iap2_source().await?;
  let pid = 0x5EA3u64;
  let transfer_id = 9u8;
  let art_id = format!("iap2/art/{pid:016x}/{transfer_id}");

  source
    .push_now_playing(Iap2NowPlaying {
      media_item: Some(Iap2MediaItem {
        persistent_id: Some(pid),
        title: Some("Cover Art Track".into()),
        artwork_id: Some(transfer_id),
        ..Default::default()
      }),
      playback: None,
    })
    .await?;
  source
    .push_now_playing(Iap2NowPlaying {
      media_item: Some(Iap2MediaItem {
        persistent_id: Some(pid),
        title: Some("Cover Art Track".into()),
        ..Default::default()
      }),
      playback: None,
    })
    .await?;
  source.push_artwork(transfer_id, vec![0xFF; 1024]).await?;

  let observed = frames.wait_for(NOW_PLAYING_WAIT, |f| f.json().contains(&art_id)).await;
  anyhow::ensure!(
    observed.is_some(),
    "resolved cover-art id {art_id} never reached the frame-tap"
  );
  Ok(())
}

/// The iOS idle sentinel (persistent_id 0 + empty title) must be fully
/// suppressed: no idle art url is ever broadcast, even if bytes arrive for it.
async fn idle_sentinel_never_broadcasts_art_url<T>(tier: &T) -> anyhow::Result<()>
where
  T: Iap2SourceDriver + FrameObserve + ModernClientDriver,
{
  let mut frames = observe_with_registered_client(tier).await?;
  let source = tier.iap2_source().await?;
  source
    .push_now_playing(Iap2NowPlaying {
      media_item: Some(Iap2MediaItem {
        persistent_id: Some(0),
        title: Some(String::new()),
        artwork_id: Some(3),
        ..Default::default()
      }),
      playback: None,
    })
    .await?;
  source.push_artwork(3, vec![0xFF; 64]).await?;

  let leaked = frames
    .wait_for(Duration::from_secs(2), |f| {
      f.json().contains("iap2/art/0000000000000000")
    })
    .await;
  anyhow::ensure!(leaked.is_none(), "idle-sentinel art url leaked to the frame-tap");
  Ok(())
}

/// A non-music app (e.g. YouTube) sends persistent_id 0 with a REAL title. The
/// suppression predicate is conjunctive (idle pid AND empty title), so this is
/// NOT the idle sentinel and the track must surface.
async fn non_music_pid_zero_with_title_surfaces<T>(tier: &T) -> anyhow::Result<()>
where
  T: Iap2SourceDriver + FrameObserve + ModernClientDriver,
{
  let mut frames = observe_with_registered_client(tier).await?;
  let source = tier.iap2_source().await?;
  source
    .push_now_playing(Iap2NowPlaying {
      media_item: Some(Iap2MediaItem {
        persistent_id: Some(0),
        title: Some("Big Buck Bunny".into()),
        ..Default::default()
      }),
      playback: None,
    })
    .await?;

  let observed = frames
    .wait_for(NOW_PLAYING_WAIT, |f| f.json().contains("Big Buck Bunny"))
    .await;
  anyhow::ensure!(
    observed.is_some(),
    "non-music pid-0 track with a real title never surfaced"
  );
  Ok(())
}

/// A non-music app's cover art must reach the webapp. iOS announces a pid-0 track
/// with a real title + an artwork transfer id, then delivers the bytes on the
/// carry-forward delta (pid absent). The resolved art id must reach the egress
/// stream and must NOT be keyed as the idle sentinel - asserted implementation-
/// agnostically (any non-sentinel `iap2/art/` url alongside the track title).
async fn non_music_artwork_reaches_frame_tap<T>(tier: &T) -> anyhow::Result<()>
where
  T: Iap2SourceDriver + FrameObserve + ModernClientDriver,
{
  let mut frames = observe_with_registered_client(tier).await?;
  let source = tier.iap2_source().await?;
  let transfer_id = 7u8;

  source
    .push_now_playing(Iap2NowPlaying {
      media_item: Some(Iap2MediaItem {
        persistent_id: Some(0),
        title: Some("Big Buck Bunny".into()),
        artwork_id: Some(transfer_id),
        ..Default::default()
      }),
      playback: None,
    })
    .await?;
  source.push_artwork(transfer_id, vec![0xFF; 1024]).await?;

  let art_seen = frames
    .wait_for(NOW_PLAYING_WAIT, |f| {
      let j = f.json();
      j.contains("Big Buck Bunny") && j.contains("iap2/art/") && !j.contains("iap2/art/0000000000000000")
    })
    .await;
  anyhow::ensure!(
    art_seen.is_some(),
    "non-music cover art never reached the frame-tap (dropped, or keyed as the idle sentinel)"
  );
  Ok(())
}

/// Outbound transport routing (TransportController). With no companion playback
/// authority, transport verbs route to the iAP2 HID path; and set_shuffle with
/// the iAP2 shuffle state unknown must refuse silently (no HID emitted) rather
/// than guess. Bound on the headless-only outbound tap, so this lifts to T1.
async fn transport_routes_to_iap2_and_refuses_unknown_shuffle<T>(tier: &T) -> anyhow::Result<()>
where
  T: CommandDriver + Iap2OutboundObserve,
{
  const PLAY_PAUSE: u8 = 0x01;
  const SHUFFLE: u8 = 0x40;

  let mut outbound = tier.iap2_outbound().await?;
  let client = tier.command_client().await?;

  // unknown iAP2 shuffle state -> the controller must refuse the toggle.
  client
    .player()
    .set_shuffle(SetShuffle { on: true })
    .await
    .expect("set_shuffle");
  // a plain transport verb with no companion authority routes to iAP2 HID.
  client.player().pause().await.expect("pause");

  let pulses = outbound.collect_for(Duration::from_secs(2)).await;
  let is_pulse = |c: &Iap2TransportCommand, bit: u8| matches!(c, Iap2TransportCommand::Hid(HidCommand::Pulse(mask)) if mask & bit != 0);
  anyhow::ensure!(
    pulses.iter().any(|c| is_pulse(c, PLAY_PAUSE)),
    "pause did not route to the iAP2 HID play/pause pulse: {pulses:?}"
  );
  anyhow::ensure!(
    !pulses.iter().any(|c| is_pulse(c, SHUFFLE)),
    "set_shuffle with unknown state must refuse, but a shuffle pulse was emitted: {pulses:?}"
  );
  Ok(())
}

/// An incoming call announced by the companion (the telephony family) must
/// surface to webapps on the egress stream. Rides the same `Gateway` on every
/// tier, so it lifts across the in-process duplex and both over-air transports.
async fn incoming_call_surfaces_to_webapp<T>(tier: &T) -> anyhow::Result<()>
where
  T: GatewayDriver + FrameObserve + ModernClientDriver,
{
  let mut frames = observe_with_registered_client(tier).await?;
  let gateway = tier.gateway().await?;
  gateway.capabilities().announce(caps()).await.expect("announce");
  gateway
    .phone()
    .call_started(PhoneCall {
      call_id: "seam-call-1".into(),
      remote_id: "+15550000001".into(),
      display_name: "Ada Lovelace".into(),
      status: PhoneCallStatus::Ringing,
      direction: PhoneCallDirection::Incoming,
      started_at_unix_s: None,
      label: None,
      address_book_id: None,
      service: None,
      is_conferenced: None,
      conference_group: None,
    })
    .await
    .expect("call_started");

  let observed = frames
    .wait_for(NOW_PLAYING_WAIT, |f| f.json().contains("Ada Lovelace"))
    .await;
  anyhow::ensure!(
    observed.is_some(),
    "incoming call never reached the webapp frame stream"
  );
  Ok(())
}

// Gateway companion rides the same `Gateway` on every tier, so this lifts
// across the in-process duplex and both over-air transports.
lift!(gateway_now_playing_reaches_frame_tap, [t1, t3_rfcomm, t3_iap2_ea]);
lift!(incoming_call_surfaces_to_webapp, [t1, t3_rfcomm, t3_iap2_ea]);

/// Geo (the aggregation family). A webapp registers a watch; the daemon forwards
/// the most-demanding watch to the companion and delivers the companion's
/// positions back to the watcher. Drives the watch with the real client SDK,
/// pushes a position from the companion, and asserts it reaches the watcher on
/// the egress stream. Binds the T1-only command driver, so it lifts to T1.
async fn geo_position_reaches_watching_webapp<T>(tier: &T) -> anyhow::Result<()>
where
  T: CommandDriver + GatewayDriver + FrameObserve,
{
  let mut frames = tier.frames().await?;
  let gateway = tier.gateway().await?;
  // the daemon refuses a watch unless the companion advertises geo support.
  let mut announce = caps();
  announce.available.geo = true;
  gateway.capabilities().announce(announce).await.expect("announce");
  let client = tier.command_client().await?;

  client
    .geo()
    .watch(GeoWatch {
      accuracy: GeoAccuracy::Fine,
      min_interval_ms: 1000,
    })
    .await
    .expect("geo watch");
  gateway
    .geo()
    .position(Position {
      lat: 12.25,
      lon: -71.5,
      alt_m: None,
      accuracy_m: 5.0,
      speed_mps: None,
      heading_deg: None,
      ts_unix_s: 1_700_000_000,
    })
    .await
    .expect("position");

  let observed = frames.wait_for(NOW_PLAYING_WAIT, |f| f.json().contains("12.25")).await;
  anyhow::ensure!(
    observed.is_some(),
    "companion geo position never reached the watching webapp"
  );
  Ok(())
}

// Outbound routing is observed via the headless-only iAP2 transport tap, so it
// lifts to T1 alone (over-air, the live session consumes the channel and the
// iPhone is the observer).
lift!(transport_routes_to_iap2_and_refuses_unknown_shuffle, [t1]);

// Geo binds the T1-only command driver (a webapp issues the watch), so T1 only.
lift!(geo_position_reaches_watching_webapp, [t1]);

// The iAP2 control-session source is event-inject at T1 and the device-half
// emulator at T3; both speak the same `Iap2Source`.
lift!(iap2_source_now_playing_reaches_frame_tap, [t1, t3_emulator]);
lift!(single_source_artwork_reaches_frame_tap, [t1, t3_emulator]);
lift!(idle_sentinel_never_broadcasts_art_url, [t1, t3_emulator]);
lift!(non_music_pid_zero_with_title_surfaces, [t1, t3_emulator]);
lift!(non_music_artwork_reaches_frame_tap, [t1, t3_emulator]);
lift!(spotify_pid_none_two_tracks_get_distinct_art_keys, [t1, t3_emulator]);
lift!(position_resets_across_track_change, [t1, t3_emulator]);

/// Field-reported: Spotify on iOS sends NowPlaying deltas with persistent_id absent
/// (Spotify does not expose MPMediaItemPropertyPersistentID). Two consecutive
/// Spotify-shape tracks with distinct title+artist but a re-used iAP2 transfer id
/// must yield DISTINCT art keys on the egress stream - the daemon currently
/// collapses every pid-absent track to the literal "nonmusic" slot, so both
/// tracks share one asset url and track A's art bytes are clobbered by track B.
async fn spotify_pid_none_two_tracks_get_distinct_art_keys<T>(tier: &T) -> anyhow::Result<()>
where
  T: Iap2SourceDriver + FrameObserve + ModernClientDriver,
{
  let mut frames = observe_with_registered_client(tier).await?;
  let source = tier.iap2_source().await?;
  let transfer_id = 9u8;

  source
    .push_now_playing(Iap2NowPlaying {
      media_item: Some(Iap2MediaItem {
        persistent_id: None,
        title: Some("Sanguirush".into()),
        artist: Some("The Destruction Of The Cult Of The Sun".into()),
        artwork_id: Some(transfer_id),
        ..Default::default()
      }),
      playback: None,
    })
    .await?;
  source.push_artwork(transfer_id, vec![0xAA; 1024]).await?;

  let track_a = frames
    .wait_for(NOW_PLAYING_WAIT, |f| {
      let j = f.json();
      j.contains("Sanguirush") && j.contains("iap2/art/")
    })
    .await;
  let track_a = track_a.ok_or_else(|| anyhow::anyhow!("track A art never reached the frame-tap"))?;
  let art_a = extract_substring_starting_with(track_a.json(), "iap2/art/")
    .ok_or_else(|| anyhow::anyhow!("track A frame had no iap2/art/ url"))?;

  source
    .push_now_playing(Iap2NowPlaying {
      media_item: Some(Iap2MediaItem {
        persistent_id: None,
        title: Some("Counterweight".into()),
        artist: Some("Other Artist".into()),
        artwork_id: Some(transfer_id),
        ..Default::default()
      }),
      playback: None,
    })
    .await?;
  source.push_artwork(transfer_id, vec![0xBB; 1024]).await?;

  let track_b = frames
    .wait_for(NOW_PLAYING_WAIT, |f| {
      let j = f.json();
      j.contains("Counterweight") && j.contains("iap2/art/")
    })
    .await;
  let track_b = track_b.ok_or_else(|| anyhow::anyhow!("track B art never reached the frame-tap"))?;
  let art_b = extract_substring_starting_with(track_b.json(), "iap2/art/")
    .ok_or_else(|| anyhow::anyhow!("track B frame had no iap2/art/ url"))?;

  anyhow::ensure!(
    art_a != art_b,
    "two distinct pid=None tracks shared one art key ({art_a}) - the daemon collapsed them to the same nonmusic slot"
  );
  Ok(())
}

/// Field-reported: iOS sends a position delta for the OLD track immediately
/// before the new media_item arrives, so when the daemon processes the track
/// change it carries the old position forward onto the new track until the
/// next position delta lands. The new track must NOT surface at the prior
/// track's progress - position must reset on a track change.
async fn position_resets_across_track_change<T>(tier: &T) -> anyhow::Result<()>
where
  T: Iap2SourceDriver + FrameObserve + ModernClientDriver,
{
  use bridgething_iap2::csm::now_playing::{PlaybackAttributes, PlaybackState};

  let mut frames = observe_with_registered_client(tier).await?;
  let source = tier.iap2_source().await?;

  // track A, three minutes in, playing.
  source
    .push_now_playing(Iap2NowPlaying {
      media_item: Some(Iap2MediaItem {
        persistent_id: Some(0x1111),
        title: Some("ThreeMinuteTrack".into()),
        duration_ms: Some(240_000),
        ..Default::default()
      }),
      playback: Some(PlaybackAttributes {
        state: Some(PlaybackState::Playing),
        position_ms: Some(180_000),
        ..Default::default()
      }),
    })
    .await?;
  let _ = frames
    .wait_for(NOW_PLAYING_WAIT, |f| f.json().contains("ThreeMinuteTrack"))
    .await
    .ok_or_else(|| anyhow::anyhow!("track A never reached the frame-tap"))?;

  // track B arrives with NO playback section, the exact shape iOS sends on a
  // fast track change (media_item first, position follows on a later delta).
  source
    .push_now_playing(Iap2NowPlaying {
      media_item: Some(Iap2MediaItem {
        persistent_id: Some(0x2222),
        title: Some("FreshTrack".into()),
        duration_ms: Some(200_000),
        ..Default::default()
      }),
      playback: None,
    })
    .await?;

  // Collect frames for track B and inspect the broadcast position. The frame
  // tap publishes the merged player state, so the position field reflects what
  // the webapp renders.
  let frame_b = frames
    .wait_for(NOW_PLAYING_WAIT, |f| f.json().contains("FreshTrack"))
    .await
    .ok_or_else(|| anyhow::anyhow!("track B never reached the frame-tap"))?;
  let position = position_ms_from_frame_json(frame_b.json())
    .ok_or_else(|| anyhow::anyhow!("could not parse position_ms from track B frame: {}", frame_b.json()))?;
  anyhow::ensure!(
    position < 10_000,
    "new track surfaced at {position} ms (carried over from the prior track at 180000 ms)"
  );
  Ok(())
}

/// Pull a contiguous non-whitespace substring out of `haystack` that starts with
/// `prefix`. Used to grab an `iap2/art/...` asset url out of an egress JSON
/// frame without committing to a fixed key shape.
fn extract_substring_starting_with(haystack: &str, prefix: &str) -> Option<String> {
  let start = haystack.find(prefix)?;
  let tail = &haystack[start..];
  let end = tail
    .find(|c: char| c == '"' || c == '\\' || c.is_whitespace())
    .unwrap_or(tail.len());
  Some(tail[..end].to_string())
}

/// Walk every `"positionMs"` (modern wire) and `"position_ms"` (stock wire)
/// occurrence in the JSON and return the largest. The frame may carry several
/// nested player snapshots; the bug surface is "did ANY of them claim 180000",
/// so we pick the worst case.
fn position_ms_from_frame_json(json: &str) -> Option<u64> {
  let mut largest: Option<u64> = None;
  for key in ["\"positionMs\"", "\"position_ms\""] {
    for (idx, _) in json.match_indices(key) {
      let tail = &json[idx + key.len()..];
      let Some(after_colon) = tail.find(':') else {
        continue;
      };
      let after = tail[after_colon + 1..].trim_start();
      let end = after.find(|c: char| !c.is_ascii_digit()).unwrap_or(after.len());
      if end == 0 {
        continue;
      }
      if let Ok(value) = after[..end].parse::<u64>() {
        largest = Some(largest.map_or(value, |prev| prev.max(value)));
      }
    }
  }
  largest
}
