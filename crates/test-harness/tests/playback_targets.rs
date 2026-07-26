use std::time::Duration;

use bridgething_gateway::Gateway;
use bridgething_test_harness::Harness;
use libbridgething::{
  CompanionAuthorityScope, GatewayCapabilities, GatewayInfo, MediaItem, Playback, PlaybackState as WirePlaybackState,
  PlaybackTarget, PlaybackTargetKind, PlayerState,
  gateway::{AuthorityClaim, PlaybackTargets},
};

const CONVERGE: Duration = Duration::from_secs(5);

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

fn target(id: &str, name: &str, kind: PlaybackTargetKind, active: bool) -> PlaybackTarget {
  PlaybackTarget {
    id: id.into(),
    name: name.into(),
    kind,
    is_active: active,
    volume_percent: Some(40),
  }
}

fn kitchen_and_desk() -> PlaybackTargets {
  PlaybackTargets {
    targets: vec![
      target("speaker", "Kitchen", PlaybackTargetKind::Speaker, true),
      target("laptop", "Desk", PlaybackTargetKind::Computer, false),
    ],
  }
}

fn remote_snapshot() -> PlayerState {
  PlayerState {
    track: Some(MediaItem {
      persistent_id: Some("spotify:track:now".into()),
      title: Some("Now Playing".into()),
      artist: Some("Artist".into()),
      duration_ms: Some(240_000),
      ..MediaItem::default()
    }),
    playback: Playback {
      state: WirePlaybackState::Playing,
      position_ms: 5_000,
      ..Playback::default()
    },
    target: Some(target("speaker", "Kitchen", PlaybackTargetKind::Speaker, true)),
    ..PlayerState::default()
  }
}

async fn claim(phone: &Gateway) {
  for scope in [
    CompanionAuthorityScope::NowPlayingMetadata,
    CompanionAuthorityScope::NowPlayingPlayback,
  ] {
    phone
      .authority()
      .claim(AuthorityClaim {
        scope,
        app_bundle: Some("com.spotify.client".into()),
      })
      .await
      .expect("claim");
  }
}

async fn connect_companion(harness: &Harness) -> Gateway {
  let phone = harness.connect_android().await.expect("companion connect");
  phone.capabilities().announce(caps()).await.expect("announce");
  phone
}

async fn target_ids(harness: &Harness) -> Vec<String> {
  harness
    .state()
    .playback_targets
    .current()
    .targets
    .into_iter()
    .map(|t| t.id)
    .collect()
}

#[tokio::test]
async fn a_companion_push_becomes_the_served_target_list() {
  let harness = Harness::start().await.expect("harness start");
  let phone = connect_companion(&harness).await;
  phone
    .player()
    .targets_changed(kitchen_and_desk())
    .await
    .expect("targets");

  let visible = harness
    .wait_for(|s| s.playback_targets.current().targets.len() == 2, CONVERGE)
    .await;
  assert!(visible, "the pushed target list never became visible");
  assert_eq!(target_ids(&harness).await, ["speaker", "laptop"]);
}

#[tokio::test]
async fn a_later_push_replaces_rather_than_merges() {
  let harness = Harness::start().await.expect("harness start");
  let phone = connect_companion(&harness).await;
  phone
    .player()
    .targets_changed(kitchen_and_desk())
    .await
    .expect("targets");
  let visible = harness
    .wait_for(|s| s.playback_targets.current().targets.len() == 2, CONVERGE)
    .await;
  assert!(visible, "first push never landed");

  phone
    .player()
    .targets_changed(PlaybackTargets {
      targets: vec![target("laptop", "Desk", PlaybackTargetKind::Computer, true)],
    })
    .await
    .expect("targets");

  let replaced = harness
    .wait_for(
      |s| {
        s.playback_targets
          .current()
          .targets
          .iter()
          .map(|t| t.id.as_str())
          .eq(["laptop"])
      },
      CONVERGE,
    )
    .await;
  assert!(
    replaced,
    "an endpoint that went away must disappear; got {:?}",
    target_ids(&harness).await
  );
}

#[tokio::test]
async fn a_companion_disconnect_clears_the_target_list() {
  let harness = Harness::start().await.expect("harness start");
  let phone = connect_companion(&harness).await;
  phone
    .player()
    .targets_changed(kitchen_and_desk())
    .await
    .expect("targets");
  let visible = harness
    .wait_for(|s| !s.playback_targets.current().targets.is_empty(), CONVERGE)
    .await;
  assert!(visible, "targets never landed while connected");

  drop(phone);
  let cleared = harness
    .wait_for(|s| s.playback_targets.current().targets.is_empty(), CONVERGE)
    .await;
  assert!(
    cleared,
    "endpoints must not outlive the companion that can transfer to them"
  );
}

#[tokio::test]
async fn the_snapshot_target_readout_reaches_the_player_state() {
  let harness = Harness::start().await.expect("harness start");
  let phone = connect_companion(&harness).await;
  claim(&phone).await;
  phone.player().snapshot(remote_snapshot()).await.expect("snapshot");

  let named = harness
    .wait_for(
      |s| s.player.state_reply().state.target.as_ref().map(|t| t.name.clone()) == Some("Kitchen".into()),
      CONVERGE,
    )
    .await;
  assert!(
    named,
    "the companion's active endpoint never reached the webapp-facing player state"
  );
}

#[tokio::test]
async fn the_target_readout_drops_when_the_companion_loses_authority() {
  let harness = Harness::start().await.expect("harness start");
  let phone = connect_companion(&harness).await;
  claim(&phone).await;
  phone.player().snapshot(remote_snapshot()).await.expect("snapshot");
  let named = harness
    .wait_for(|s| s.player.state_reply().state.target.is_some(), CONVERGE)
    .await;
  assert!(named, "readout never landed while authoritative");

  phone
    .authority()
    .release(libbridgething::gateway::AuthorityRelease {
      scope: CompanionAuthorityScope::NowPlayingMetadata,
    })
    .await
    .expect("release");

  let dropped = harness
    .wait_for(|s| s.player.state_reply().state.target.is_none(), CONVERGE)
    .await;
  assert!(
    dropped,
    "a stale endpoint must not keep claiming the card once iap2 owns the view"
  );
}
