//! iAP2 (iOS) scenarios driven in-process by injecting `SessionEvent`s into
//! the same channel the real `Iap2Manager` feeds. The unmodified router does
//! the merge, stock translation, and broadcast; only the radio + MFi are
//! absent (those are the T3 over-the-air lane). The events here are exactly
//! what a real iPhone's iAP2 session produces.

use std::time::Duration;

use bridgething::ClientMode;
use bridgething_iap2::{
  SessionEvent,
  csm::now_playing::{MediaItemAttributes, NowPlayingUpdate, PlaybackAttributes, encode_queue_snapshot},
};
use bridgething_test_harness::Harness;
use libbridgething::{GatewayCapabilities, GatewayInfo, QueueItem, gateway::NowPlayingEnrichment};

const CONVERGE: Duration = Duration::from_secs(3);

fn companion_caps() -> GatewayCapabilities {
  GatewayCapabilities {
    gateway: GatewayInfo {
      address: String::new(),
      name: "harness-companion".into(),
      os_name: "ios".into(),
      app_name: "harness-companion".into(),
      app_version: "0.0.0".into(),
      adapter_version: "harness".into(),
      lib_version: "0.0.0".into(),
      libbridgething_version: format!("v{}", libbridgething::LIBBRIDGETHING_VERSION),
    },
    ..Default::default()
  }
}

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

/// The queue snapshot path: a now-playing delta with `queue_list_avail` + a
/// transfer id arms the daemon, then the FileTransfer blob (a CSM param block of
/// wrapped media items) decodes into the merged queue. Flipping `queue_list_avail`
/// to false clears it. Exercises the public `encode_queue_snapshot` codec, the
/// router's per-peer queue context, and `decode_queue_snapshot`.
#[tokio::test]
async fn iap2_queue_snapshot_populates_then_clears_on_avail_flip() {
  let harness = Harness::start().await.expect("harness start");
  let phone = harness.iap2_peer();
  let transfer_id = 7u8;

  // arm: advertise an available queue list under transfer id 7.
  harness
    .inject_iap2(
      phone,
      SessionEvent::NowPlayingUpdate(NowPlayingUpdate {
        media_item: Some(MediaItemAttributes {
          persistent_id: Some(0xAAA0),
          title: Some("Now Playing".into()),
          ..Default::default()
        }),
        playback: Some(PlaybackAttributes {
          queue_list_avail: Some(true),
          queue_list_transfer_id: Some(transfer_id),
          ..Default::default()
        }),
      }),
    )
    .await
    .expect("arm queue list");

  let snapshot = encode_queue_snapshot(vec![
    MediaItemAttributes {
      persistent_id: Some(0xAAA1),
      title: Some("Queue One".into()),
      artist: Some("Q Artist".into()),
      ..Default::default()
    },
    MediaItemAttributes {
      persistent_id: Some(0xAAA2),
      title: Some("Queue Two".into()),
      ..Default::default()
    },
  ]);
  harness
    .inject_iap2(
      phone,
      SessionEvent::QueueSnapshotBytes {
        transfer_id,
        bytes: snapshot,
      },
    )
    .await
    .expect("deliver queue snapshot");

  let populated = harness
    .wait_for(|state| state.player.queue_reply().items.len() == 2, CONVERGE)
    .await;
  assert!(populated, "queue snapshot never decoded into a 2-item queue");
  let items = harness.state().player.queue_reply().items;
  assert_eq!(items[0].uri, "iap2:track:000000000000aaa1");
  assert_eq!(items[0].title.as_deref(), Some("Queue One"));
  assert_eq!(items[1].title.as_deref(), Some("Queue Two"));

  // flip availability off: the queue must clear.
  harness
    .inject_iap2(
      phone,
      SessionEvent::NowPlayingUpdate(NowPlayingUpdate {
        media_item: None,
        playback: Some(PlaybackAttributes {
          queue_list_avail: Some(false),
          ..Default::default()
        }),
      }),
    )
    .await
    .expect("flip queue_list_avail off");

  let cleared = harness
    .wait_for(|state| state.player.queue_reply().items.is_empty(), CONVERGE)
    .await;
  assert!(cleared, "queue did not clear when queue_list_avail flipped to false");
}

/// End-to-end iOS enrichment: iAP2 establishes the identity, then a non-
/// authoritative companion (the iOS shape - announces but claims no scope)
/// sends an `EnrichmentOffer` over the real gateway duplex. The daemon must
/// keep the iAP2 identity, overlay the spotify uri + art, light the heart
/// (is_like_supported), and overlay the queue - proving the whole wire path:
/// gateway dispatch -> apply_enrichment -> merge -> client state reply.
#[tokio::test]
async fn iap2_enrichment_overlay_resolves_uri_art_heart_and_queue() {
  let harness = Harness::start().await.expect("harness start");
  let phone_addr = harness.iap2_peer();
  let companion = harness.connect_android().await.expect("connect companion");
  companion
    .capabilities()
    .announce(companion_caps())
    .await
    .expect("announce");

  // iAP2 identity for pid 0x1234 renders as "iap2:track:0000000000001234".
  harness
    .inject_iap2(
      phone_addr,
      SessionEvent::NowPlayingUpdate(NowPlayingUpdate {
        media_item: Some(MediaItemAttributes {
          persistent_id: Some(0x1234),
          title: Some("Enriched Song".into()),
          artist: Some("Enriched Artist".into()),
          duration_ms: Some(180_000),
          ..Default::default()
        }),
        playback: None,
      }),
    )
    .await
    .expect("inject iap2 now-playing");

  companion
    .player()
    .enrichment_offer(NowPlayingEnrichment {
      anchor_pid: Some("iap2:track:0000000000001234".into()),
      head: Some(QueueItem {
        uri: "spotify:track:gold".into(),
        title: Some("Enriched Song".into()),
        artist: Some("Enriched Artist".into()),
        album: None,
        artwork_id: Some("spotify/img/gold".into()),
        duration_ms: Some(180_000),
        persistent_id: None,
      }),
      queue: vec![QueueItem {
        uri: "spotify:track:next".into(),
        title: Some("Up Next".into()),
        artist: Some("Enriched Artist".into()),
        album: None,
        artwork_id: Some("spotify/img/next".into()),
        duration_ms: Some(200_000),
        persistent_id: None,
      }],
      context: None,
    })
    .await
    .expect("send enrichment offer");

  let converged = harness
    .wait_for(
      |state| state.player.state_reply().state.track.and_then(|t| t.uri).as_deref() == Some("spotify:track:gold"),
      CONVERGE,
    )
    .await;
  assert!(converged, "enrichment uri overlay never reached merged state");

  let reply = harness.state().player.state_reply();
  let track = reply.state.track.expect("track present");
  assert_eq!(
    track.persistent_id.as_deref(),
    Some("iap2:track:0000000000001234"),
    "identity stays iAP2"
  );
  assert_eq!(track.title.as_deref(), Some("Enriched Song"), "title stays iAP2");
  assert_eq!(track.uri.as_deref(), Some("spotify:track:gold"), "spotify uri overlaid");
  assert_eq!(
    track.artwork_id.as_deref(),
    Some("spotify/img/gold"),
    "spotify art overlaid"
  );
  assert_eq!(track.is_like_supported, Some(true), "heart lit on exact match");
  assert_eq!(
    reply.state.queue.first().map(|q| q.uri.as_str()),
    Some("spotify:track:next"),
    "queue overlaid from the offer"
  );
}
