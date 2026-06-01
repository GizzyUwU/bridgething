//! Model-based property engine - Mode 1: deterministic
//! differential. A proptest strategy generates random sequences over the atomic
//! inbound vocabulary; each event is applied to both the real headless daemon
//! and the reference [`Model`], and after every event the daemon's merged
//! projection must equal the model's. A divergence is either a daemon bug or a
//! model bug - both worth knowing.
//!
//! The vocabulary spans both sources: iAP2 deltas + artwork (where the cover-art
//! family lives - the production flicker was a no-companion phone) and companion
//! deltas + authority claim/release (the authority-gated merge: fallthrough and
//! the artwork-no-fallthrough rule). Companion disconnect and the chaos /
//! invariants mode land next.
//!
//! Time is frozen out of scope: the vocabulary is inbound-only (no transport or
//! seek intents armed), sequences run sub-second (authority never goes stale),
//! and the extrapolated `position_ms` is not in the compared projection. Each
//! case gets a fresh `Harness` (the Player is a single global actor with no
//! reset, so peer-id namespacing cannot isolate it) on its own runtime so a
//! dropped daemon's detached tasks die with the runtime.
//!
//! The barrier is combined: after every event we wait for the daemon's player
//! projection AND its live authority scopes to match the model. The authority
//! half is load-bearing - a bare claim has no player-observable effect, and the
//! companion handler spawns per message, so without it a claim would race a
//! following companion delta.

use std::{collections::HashSet, time::Duration};

use bluer::Address;
use bridgething::State;
use bridgething_gateway::Gateway;
use bridgething_iap2::{
  SessionEvent,
  csm::now_playing::{
    MediaItemAttributes, NowPlayingUpdate as Iap2NowPlaying, PlaybackAttributes, PlaybackState, RepeatMode, ShuffleMode,
  },
};
use bridgething_test_harness::{
  Harness,
  model::{Model, ModelEvent, Projection},
};
use libbridgething::{
  CompanionAuthorityScope, GatewayCapabilities, GatewayInfo, MediaItemUpdate, NowPlayingUpdate, PlaybackUpdate,
  gateway::{AuthorityClaim, AuthorityRelease},
};
use proptest::prelude::*;

const CONVERGE: Duration = Duration::from_millis(500);
/// The player actor must produce no snapshot tick for this long before the
/// daemon is considered quiescent (no command in flight).
const STABLE_WINDOW: Duration = Duration::from_millis(20);

fn daemon_projection(state: &State) -> Projection {
  Projection::from_daemon(
    &state.player.state_reply(),
    &state.player.queue_reply(),
    state.player.current_artwork_id(),
  )
}

fn daemon_authority(state: &State) -> HashSet<CompanionAuthorityScope> {
  state.authority.live_scopes().into_iter().collect()
}

// the engine compares the full correct projection. the held bug #1 (the
// pid-hex overload) makes this diverge - on the artwork ids and on track_id (a
// dropped-art recompute leaves a stale cache-frozen track) - and that red is
// the point: it documents the bug and proves the engine's reach. it goes green
// when bug #1 is fixed. do not mask fields to keep it green.
fn projection_matches(
  state: &State,
  expected: &Projection,
  expected_authority: &HashSet<CompanionAuthorityScope>,
) -> bool {
  daemon_projection(state) == *expected && daemon_authority(state) == *expected_authority
}

/// Wait until the daemon matches the model AND the player actor has gone quiet
/// (no snapshot tick for `STABLE_WINDOW`), so no command is in flight when the
/// caller applies the next event. Generic over the watch payload to avoid naming
/// the snapshot type.
async fn converge<T>(
  harness: &Harness,
  player_watch: &mut tokio::sync::watch::Receiver<T>,
  expected: &Projection,
  expected_authority: &HashSet<CompanionAuthorityScope>,
) -> bool {
  let deadline = tokio::time::Instant::now() + CONVERGE;
  loop {
    if projection_matches(harness.state(), expected, expected_authority) {
      player_watch.mark_unchanged();
      match tokio::time::timeout(STABLE_WINDOW, player_watch.changed()).await {
        // window elapsed (or actor gone): quiescent - confirm still matched
        Err(_) | Ok(Err(_)) => {
          if projection_matches(harness.state(), expected, expected_authority) {
            return true;
          }
        }
        // a tick arrived: a command landed, re-check
        Ok(Ok(())) => {}
      }
    } else {
      let _ = tokio::time::timeout(Duration::from_millis(10), player_watch.changed()).await;
    }
    if tokio::time::Instant::now() >= deadline {
      return projection_matches(harness.state(), expected, expected_authority);
    }
  }
}

/// Minimal announce so the daemon registers + marks the companion connected.
fn caps() -> GatewayCapabilities {
  GatewayCapabilities {
    gateway: GatewayInfo {
      address: String::new(),
      name: "harness-companion".into(),
      os_name: "android".into(),
      app_name: "harness-companion".into(),
      app_version: "0.0.0".into(),
      adapter_version: "harness".into(),
      lib_version: "0.0.0".into(),
      libbridgething_version: format!("v{}", libbridgething::LIBBRIDGETHING_VERSION),
    },
    ..Default::default()
  }
}

async fn apply_to_daemon(harness: &Harness, addr: Address, phone: &Gateway, event: &ModelEvent) -> anyhow::Result<()> {
  match event {
    ModelEvent::Iap2NowPlaying(update) => {
      harness
        .inject_iap2(addr, SessionEvent::NowPlayingUpdate(update.clone()))
        .await
    }
    ModelEvent::Iap2Artwork { transfer_id, bytes_len } => {
      harness.iap2_artwork(addr, *transfer_id, vec![0u8; *bytes_len]).await
    }
    ModelEvent::CompanionDelta(update) => phone
      .player()
      .delta(update.clone())
      .await
      .map_err(|e| anyhow::anyhow!("companion delta: {e}")),
    ModelEvent::AuthorityClaim(scope) => phone
      .authority()
      .claim(AuthorityClaim { scope: *scope })
      .await
      .map_err(|e| anyhow::anyhow!("authority claim: {e}")),
    ModelEvent::AuthorityRelease(scope) => phone
      .authority()
      .release(AuthorityRelease { scope: *scope })
      .await
      .map_err(|e| anyhow::anyhow!("authority release: {e}")),
  }
}

// --- strategies: a small value space so track changes, transfer-id reuse, and
// idle deltas recur within a short sequence (the interesting collisions). ---

// a media_item with no persistent_id is the iAP2 carry-forward delta. it trips
// the held bug #1 (double-prefixed track id), so generating it makes the engine
// red - which is correct: that input is real and the daemon mishandles it today.
fn pid_strategy() -> impl Strategy<Value = Option<u64>> {
  prop_oneof![
    Just(None),
    Just(Some(0u64)),
    Just(Some(1)),
    Just(Some(2)),
    Just(Some(3))
  ]
}

fn title_strategy() -> impl Strategy<Value = Option<String>> {
  prop_oneof![
    Just(None),
    Just(Some(String::new())),
    Just(Some("Alpha".to_string())),
    Just(Some("Beta".to_string())),
  ]
}

fn name_strategy() -> impl Strategy<Value = Option<String>> {
  prop_oneof![Just(None), Just(Some("X".to_string())), Just(Some("Y".to_string()))]
}

fn opt_transfer_id() -> impl Strategy<Value = Option<u8>> {
  prop_oneof![Just(None), (1u8..=3).prop_map(Some)]
}

fn opt_duration() -> impl Strategy<Value = Option<u32>> {
  prop_oneof![Just(None), Just(Some(120_000u32)), Just(Some(240_000))]
}

fn opt_bool() -> impl Strategy<Value = Option<bool>> {
  prop_oneof![Just(None), Just(Some(true)), Just(Some(false))]
}

fn media_strategy() -> impl Strategy<Value = Option<MediaItemAttributes>> {
  prop_oneof![
    1 => Just(None),
    6 => (
      pid_strategy(),
      title_strategy(),
      opt_transfer_id(),
      opt_duration(),
      opt_bool(),
      name_strategy(),
      name_strategy(),
    )
      .prop_map(|(persistent_id, title, artwork_id, duration_ms, liked, album, artist)| {
        Some(MediaItemAttributes {
          persistent_id,
          title,
          artwork_id,
          duration_ms,
          liked,
          album,
          artist,
          ..Default::default()
        })
      }),
  ]
}

fn opt_state() -> impl Strategy<Value = Option<PlaybackState>> {
  prop_oneof![
    Just(None),
    Just(Some(PlaybackState::Playing)),
    Just(Some(PlaybackState::Paused)),
  ]
}

fn opt_shuffle() -> impl Strategy<Value = Option<ShuffleMode>> {
  prop_oneof![Just(None), Just(Some(ShuffleMode::Off)), Just(Some(ShuffleMode::Songs)),]
}

fn opt_repeat() -> impl Strategy<Value = Option<RepeatMode>> {
  prop_oneof![
    Just(None),
    Just(Some(RepeatMode::Off)),
    Just(Some(RepeatMode::Track)),
    Just(Some(RepeatMode::All)),
  ]
}

fn playback_strategy() -> impl Strategy<Value = Option<PlaybackAttributes>> {
  prop_oneof![
    1 => Just(None),
    4 => (opt_state(), opt_shuffle(), opt_repeat(), opt_bool()).prop_map(
      |(state, shuffle_mode, repeat, set_elapsed_time_available)| {
        Some(PlaybackAttributes {
          state,
          shuffle_mode,
          repeat,
          set_elapsed_time_available,
          ..Default::default()
        })
      }
    ),
  ]
}

fn companion_pid() -> impl Strategy<Value = Option<String>> {
  prop_oneof![
    Just(Some("spotify:1".to_string())),
    Just(Some("spotify:2".to_string())),
    Just(Some("spotify:3".to_string())),
  ]
}

// Companion deltas carry no artwork_id in this cut: companion-pushed assets ride
// a separate AssetPush the model does not track, so a companion art id would
// trip the dangling-art invariant. Omitting it still exercises the key rule -
// companion-authoritative-with-no-art must NOT fall through to iAP2 art.
fn companion_delta_strategy() -> impl Strategy<Value = NowPlayingUpdate> {
  let media = prop_oneof![
    1 => Just(None),
    5 => (companion_pid(), title_strategy(), opt_duration(), opt_bool(), name_strategy()).prop_map(
      |(persistent_id, title, duration_ms, liked, album)| {
        Some(MediaItemUpdate {
          persistent_id,
          title,
          duration_ms,
          liked,
          album,
          ..Default::default()
        })
      }
    ),
  ];
  let playback = prop_oneof![
    1 => Just(None),
    3 => (opt_state(), opt_shuffle(), opt_repeat(), opt_bool()).prop_map(|(state, shuffle_mode, repeat, set_elapsed)| {
      Some(PlaybackUpdate {
        playing: state.map(|s| matches!(s, PlaybackState::Playing)),
        shuffle: shuffle_mode.map(|m| m.is_on()),
        shuffle_mode: shuffle_mode.map(translate_shuffle),
        repeat: repeat.map(translate_repeat),
        set_elapsed_time_available: set_elapsed,
        ..Default::default()
      })
    }),
  ];
  (media, playback).prop_map(|(media_item, playback)| NowPlayingUpdate { media_item, playback })
}

fn scope_strategy() -> impl Strategy<Value = CompanionAuthorityScope> {
  prop_oneof![
    Just(CompanionAuthorityScope::NowPlayingMetadata),
    Just(CompanionAuthorityScope::NowPlayingPlayback),
  ]
}

// these mirror the daemon's iAP2 -> lib translation so companion playback deltas
// can be expressed in the same small value space as the iAP2 ones.
fn translate_shuffle(mode: ShuffleMode) -> libbridgething::ShuffleMode {
  match mode {
    ShuffleMode::Off => libbridgething::ShuffleMode::Off,
    ShuffleMode::Songs => libbridgething::ShuffleMode::Songs,
    ShuffleMode::Albums => libbridgething::ShuffleMode::Albums,
  }
}

fn translate_repeat(mode: RepeatMode) -> libbridgething::RepeatMode {
  match mode {
    RepeatMode::Off => libbridgething::RepeatMode::Off,
    RepeatMode::Track => libbridgething::RepeatMode::One,
    RepeatMode::All => libbridgething::RepeatMode::All,
  }
}

fn event_strategy() -> impl Strategy<Value = ModelEvent> {
  prop_oneof![
    4 => (media_strategy(), playback_strategy())
      .prop_map(|(media_item, playback)| ModelEvent::Iap2NowPlaying(Iap2NowPlaying { media_item, playback })),
    2 => (1u8..=3).prop_map(|transfer_id| ModelEvent::Iap2Artwork { transfer_id, bytes_len: 64 }),
    3 => companion_delta_strategy().prop_map(ModelEvent::CompanionDelta),
    2 => scope_strategy().prop_map(ModelEvent::AuthorityClaim),
    1 => scope_strategy().prop_map(ModelEvent::AuthorityRelease),
  ]
}

fn events_strategy() -> impl Strategy<Value = Vec<ModelEvent>> {
  prop::collection::vec(event_strategy(), 1..=18)
}

// Mode 2 (chaos) vocabulary: broader than Mode 1 - includes the idle sentinel
// (pid 0 + empty title), the YouTube case (pid 0 + real title), and no-pid
// deltas. Mode 2 does not compare full state (the daemon's interleaving is
// nondeterministic under the spawn races), so the bug-#1-poisoned paths are fine
// here; it asserts only order-independent safety invariants.
fn chaos_iap2_pid() -> impl Strategy<Value = Option<u64>> {
  prop_oneof![Just(None), Just(Some(0u64)), Just(Some(1)), Just(Some(2))]
}

fn chaos_iap2_title() -> impl Strategy<Value = Option<String>> {
  prop_oneof![
    Just(None),
    Just(Some(String::new())),
    Just(Some("Real".to_string())),
    Just(Some("Beta".to_string())),
  ]
}

fn chaos_event_strategy() -> impl Strategy<Value = ModelEvent> {
  let iap2_media = prop_oneof![
    1 => Just(None),
    5 => (chaos_iap2_pid(), chaos_iap2_title(), opt_transfer_id(), opt_duration()).prop_map(
      |(persistent_id, title, artwork_id, duration_ms)| Some(MediaItemAttributes {
        persistent_id,
        title,
        artwork_id,
        duration_ms,
        ..Default::default()
      })
    ),
  ];
  prop_oneof![
    4 => (iap2_media, playback_strategy())
      .prop_map(|(media_item, playback)| ModelEvent::Iap2NowPlaying(Iap2NowPlaying { media_item, playback })),
    2 => (0u8..=3).prop_map(|transfer_id| ModelEvent::Iap2Artwork { transfer_id, bytes_len: 64 }),
    3 => companion_delta_strategy().prop_map(ModelEvent::CompanionDelta),
    2 => scope_strategy().prop_map(ModelEvent::AuthorityClaim),
    1 => scope_strategy().prop_map(ModelEvent::AuthorityRelease),
  ]
}

fn chaos_events_strategy() -> impl Strategy<Value = Vec<ModelEvent>> {
  prop::collection::vec(chaos_event_strategy(), 4..=24)
}

/// Wait until the player actor has gone quiet (no tick for `STABLE_WINDOW`),
/// regardless of what state it settles to. Used by chaos mode, which fires a
/// burst with no per-event barrier and then lets it drain.
async fn quiesce<T>(player_watch: &mut tokio::sync::watch::Receiver<T>) {
  let deadline = tokio::time::Instant::now() + CONVERGE;
  loop {
    player_watch.mark_unchanged();
    match tokio::time::timeout(STABLE_WINDOW, player_watch.changed()).await {
      Err(_) | Ok(Err(_)) => return,
      Ok(Ok(())) => {}
    }
    if tokio::time::Instant::now() >= deadline {
      return;
    }
  }
}

fn liveness_npu() -> Iap2NowPlaying {
  Iap2NowPlaying {
    media_item: Some(MediaItemAttributes {
      persistent_id: Some(0xABCD),
      title: Some("liveness".to_string()),
      ..Default::default()
    }),
    playback: None,
  }
}

proptest! {
  #![proptest_config(ProptestConfig { cases: 128, ..ProptestConfig::default() })]

  /// The daemon's merged projection (and live authority) equals the reference
  /// model's after every inbound event, for any sequence over the vocabulary.
  #[test]
  fn merge_matches_model(events in events_strategy()) {
    let rt = tokio::runtime::Builder::new_multi_thread()
      .worker_threads(2)
      .enable_all()
      .build()
      .unwrap();

    rt.block_on(async move {
      let harness = Harness::start().await.expect("harness start");
      let addr = harness.iap2_peer();
      let phone = harness.connect_android().await.expect("connect companion");
      phone.capabilities().announce(caps()).await.expect("announce");
      let mut player_watch = harness.state().player.snapshot_watch();
      let mut model = Model::new();

      for (i, event) in events.iter().enumerate() {
        apply_to_daemon(&harness, addr, &phone, event).await.expect("inject");
        model.apply(event);
        let expected = model.project();
        let expected_authority = model.authority_scopes();

        // Quiescence barrier. The companion handler spawns per message and peer
        // transitions fire a stray player SendState, so a command can be in flight
        // across an event boundary and race the next event (e.g. a claim). Wait
        // until the projection matches AND the player actor has gone quiet (no
        // watch tick for STABLE_WINDOW), guaranteeing no command is in flight when
        // the next event is applied - so the daemon never reorders relative to the
        // sequential model. A bare authority claim enqueues no player command, so
        // it is quiescent at once (and must leave the player projection unchanged).
        let converged = converge(&harness, &mut player_watch, &expected, &expected_authority).await;

        prop_assert!(
          converged,
          "divergence after event {i} ({event:?})\n  expected (model): {expected:#?}\n  actual (daemon):  {:#?}\n  expected authority: {expected_authority:?}\n  actual authority:   {:?}\n  full sequence: {events:#?}",
          daemon_projection(harness.state()),
          daemon_authority(harness.state()),
        );

        // every projected artwork id must reference an asset the daemon was told
        // about (inserted or pending) - no dangling/synthesized art url.
        let known = model.known_asset_ids();
        if let Some(id) = &expected.wire_artwork_id {
          prop_assert!(known.contains(id), "wire artwork id {id} not in known assets {known:?}");
        }
        // no idle-sentinel art url ever surfaces.
        if let Some(id) = &expected.wire_artwork_id {
          prop_assert!(!id.contains("0000000000000000"), "idle-sentinel art url surfaced: {id}");
        }
      }

      Ok(())
    })?;
  }
}

proptest! {
  #![proptest_config(ProptestConfig { cases: 64, ..ProptestConfig::default() })]

  /// Mode 2 - chaos. Fire a random multi-source burst with NO per-event barrier,
  /// so the daemon's per-message spawning genuinely interleaves player + authority
  /// + iAP2. The final state is nondeterministic, so assert only order-independent
  /// safety invariants: the daemon never emits an idle-sentinel art url, and the
  /// player actor is still alive after the burst (no deadlock/panic under racing).
  #[test]
  fn chaos_burst_holds_safety_invariants(events in chaos_events_strategy()) {
    let rt = tokio::runtime::Builder::new_multi_thread()
      .worker_threads(2)
      .enable_all()
      .build()
      .unwrap();

    rt.block_on(async move {
      let harness = Harness::start().await.expect("harness start");
      let addr = harness.iap2_peer();
      let phone = harness.connect_android().await.expect("connect companion");
      phone.capabilities().announce(caps()).await.expect("announce");
      let mut player_watch = harness.state().player.snapshot_watch();

      // fire the burst with no barrier - the daemon's spawning races them.
      for event in &events {
        apply_to_daemon(&harness, addr, &phone, event).await.expect("inject");
      }
      quiesce(&mut player_watch).await;

      // safety invariant: no idle-sentinel art url, whatever the interleaving.
      let proj = daemon_projection(harness.state());
      for id in [&proj.wire_artwork_id, &proj.current_artwork_id].into_iter().flatten() {
        prop_assert!(
          !id.contains("0000000000000000"),
          "idle-sentinel art url surfaced under chaos: {id}\n  sequence: {events:#?}"
        );
      }

      // liveness: the player actor still processes a fresh command after the burst.
      player_watch.mark_unchanged();
      harness
        .inject_iap2(addr, SessionEvent::NowPlayingUpdate(liveness_npu()))
        .await
        .expect("liveness inject");
      let alive = tokio::time::timeout(CONVERGE, player_watch.changed()).await.is_ok();
      prop_assert!(alive, "player actor did not process an event after the burst (deadlock/panic?)\n  sequence: {events:#?}");

      Ok(())
    })?;
  }
}
