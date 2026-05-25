//! iAP2 (iOS) scenarios driven in-process by injecting `SessionEvent`s into
//! the same channel the real `Iap2Manager` feeds. The unmodified router does
//! the merge, stock translation, and broadcast; only the radio + MFi are
//! absent (those are the T3 over-the-air lane). The events here are exactly
//! what a real iPhone's iAP2 session produces.

use std::time::Duration;

use bridgething::ClientMode;
use bridgething_iap2::{
  SessionEvent,
  csm::now_playing::{MediaItemAttributes, NowPlayingUpdate},
};
use bridgething_test_harness::Harness;

const CONVERGE: Duration = Duration::from_secs(3);

/// Inject a single-source iAP2 now-playing delta with no companion present -
/// the production cover-art-bug shape. iAP2 is the fallthrough source, so it
/// merges without an authority claim. Proves the router runs in the headless
/// lane and injected events route end-to-end into merged state.
#[tokio::test]
async fn iap2_single_source_now_playing() {
  let harness = Harness::start().await.expect("harness start");
  let phone = harness.iap2_peer();

  harness
    .inject_iap2(
      phone,
      SessionEvent::NowPlayingUpdate(NowPlayingUpdate {
        media_item: Some(MediaItemAttributes {
          persistent_id: Some(0x1234),
          title: Some("iAP2 Song".into()),
          artist: Some("iAP2 Artist".into()),
          ..Default::default()
        }),
        playback: None,
      }),
    )
    .await
    .expect("inject iap2 now-playing");

  let converged = harness
    .wait_for(
      |state| state.player.state_reply().state.track.and_then(|t| t.title).as_deref() == Some("iAP2 Song"),
      CONVERGE,
    )
    .await;
  assert!(converged, "merged title never reflected injected iAP2 data");

  let track = harness.state().player.state_reply().state.track.expect("track present");
  assert_eq!(track.artist.as_deref(), Some("iAP2 Artist"));
}

/// Inject a now-playing delta carrying an artwork transfer id, then deliver
/// the artwork bytes a beat later (the real iPhone ordering). The merged art
/// id must reach a connected client's broadcast, and the bytes must resolve
/// into the asset cache. Exercises the artwork inject + frame observer.
#[tokio::test]
async fn iap2_artwork_resolves_and_appears_in_broadcast() {
  let harness = Harness::start().await.expect("harness start");
  let mut frames = harness.observe_frames();
  let _client = harness.connect_modern_client().await.expect("connect modern client");

  // barrier: wait until the daemon has registered the client before driving
  // a broadcast at it, else the broadcast can race ahead of registration.
  let registered = harness
    .wait_for(|state| state.client_man.client_count() >= 1, CONVERGE)
    .await;
  assert!(registered, "modern client never registered");

  let phone = harness.iap2_peer();
  harness
    .inject_iap2(
      phone,
      SessionEvent::NowPlayingUpdate(NowPlayingUpdate {
        media_item: Some(MediaItemAttributes {
          persistent_id: Some(0x1234),
          title: Some("Art Song".into()),
          artwork_id: Some(7),
          ..Default::default()
        }),
        playback: None,
      }),
    )
    .await
    .expect("inject now-playing");
  harness
    .iap2_artwork(phone, 7, vec![0xFF; 64])
    .await
    .expect("inject artwork");

  let art_id = "iap2/art/0000000000001234/7";

  let observed = frames
    .wait_for(CONVERGE, |f| f.mode == ClientMode::Modern && f.json().contains(art_id))
    .await;
  assert!(observed.is_some(), "no modern broadcast carried the art id");

  let cached = harness.state().assets.get(art_id).await.expect("asset get");
  assert!(
    cached.is_some(),
    "iap2 artwork bytes never resolved into the asset cache"
  );
}
