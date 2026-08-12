#[path = "rig/backends.rs"]
mod backends;
#[path = "rig/fakes.rs"]
mod fakes;
#[path = "rig/log_sink.rs"]
mod log_sink;
#[path = "rig/secrets.rs"]
mod secrets;
#[path = "rig/support.rs"]
mod support;

use std::{path::PathBuf, sync::Arc, time::Duration};

use bridgething_companion::{
  api::{PeerLinkStatus, SessionEvent},
  provider::Provider,
};
use bridgething_delivery::ota::{event::OtaPhaseSnapshot, stream::FileSource};
use bridgething_gateway::route;
use libbridgething::{MediaItem, Playback, PlaybackState, PlayerState};
use serde::Serialize;

use crate::{
  fakes::FakeSource,
  support::{Rig, WireEntry},
};

const ARTIFACT_BYTES: usize = 512 * 1024;

const DRIVE_DEADLINE: Duration = Duration::from_secs(60);
const SETTLE: Duration = Duration::from_secs(5);

fn playing(title: &str) -> PlayerState {
  PlayerState {
    track: Some(MediaItem {
      title: Some(title.into()),
      artist: Some("the rig".into()),
      ..MediaItem::default()
    }),
    playback: Playback {
      state: PlaybackState::Playing,
      ..Playback::default()
    },
    ..PlayerState::default()
  }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_session_announces_itself_and_the_daemon_learns_what_the_host_can_do() {
  let rig = Rig::start().await;

  assert!(
    rig
      .await_event(SETTLE, |event| matches!(
        event,
        SessionEvent::PeerConnected { peer } if peer.status == PeerLinkStatus::Connected
      ))
      .await,
    "the host is told a peer connected"
  );

  assert!(
    rig
      .harness
      .wait_for(
        |state| state
          .capabilities
          .snapshot()
          .gateway
          .is_some_and(|info| info.app_name == "rig"),
        SETTLE,
      )
      .await,
    "the daemon adopted the announce and knows who the host is"
  );
}

#[tokio::test(flavor = "multi_thread")]
async fn every_inbound_request_surface_answers_within_the_deadline() {
  let rig = Rig::start().await;
  rig.settle().await;

  let peer = rig.session.peer_for(rig.device_id()).expect("the link is up");
  let gateway = rig.gateway();
  for probe in support::probes() {
    let name = probe.name;
    let answered = tokio::time::timeout(SETTLE, route(&peer, probe.msg, gateway.connection())).await;
    assert!(
      answered.is_ok(),
      "{name} left the device waiting: an inbound request surface that never returns is a hang, \
       and refusing is an answer"
    );
  }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_providers_now_playing_reaches_both_the_device_and_the_host() {
  let rig = Rig::start().await;
  let source: Arc<FakeSource> = FakeSource::new("rig");
  rig
    .session
    .add_provider(source.clone() as Arc<dyn Provider>)
    .await
    .expect("the provider attaches");
  rig.settle().await;

  source.submit(playing("the litmus track"));

  assert!(
    rig
      .await_event(SETTLE, |event| matches!(
        event,
        SessionEvent::NowPlayingChanged { now_playing: Some(now) }
          if now.track.as_ref().and_then(|track| track.title.as_deref()) == Some("the litmus track")
      ))
      .await,
    "the host hears the hub's arbitrated voice"
  );

  assert!(
    rig
      .harness
      .wait_for(
        |state| state
          .player
          .state_reply()
          .state
          .track
          .and_then(|track| track.title)
          .as_deref()
          == Some("the litmus track"),
        SETTLE,
      )
      .await,
    "and so does the daemon"
  );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_full_update_drive_completes_against_the_real_daemon() {
  let rig = Rig::start().await;
  rig.settle().await;
  let artifact = rig.write_artifact("daemon", ARTIFACT_BYTES);

  let terminal = tokio::time::timeout(
    DRIVE_DEADLINE,
    rig
      .session
      .ota()
      .push_daemon(rig.device_id(), Arc::new(FileSource::open(artifact.clone())), None),
  )
  .await
  .expect("the drive ended rather than parking on the watchdog");

  assert_eq!(
    terminal,
    OtaPhaseSnapshot::Completed,
    "the daemon staged the piece, took the activate and reported reboot"
  );

  let said = rig.said();
  assert_eq!(
    said.acked.last().copied(),
    Some(ARTIFACT_BYTES as u32),
    "the daemon acked every byte of the artifact, got {:?}",
    said.acked
  );
  assert!(
    said.acked.len() > 1,
    "a paced push is acked as it lands, not once at the end, got {:?}",
    said.acked
  );
  assert!(
    said.acked.windows(2).all(|pair| pair[0] <= pair[1]),
    "acks are cumulative and never rewind, got {:?}",
    said.acked
  );
  assert!(
    said.phases.contains(&libbridgething::OtaPhase::Writing),
    "the daemon reported the apply, got {:?}",
    said.phases
  );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_second_push_of_the_same_artifact_resumes_rather_than_restarting() {
  let rig = Rig::start().await;
  rig.settle().await;
  let artifact = rig.write_artifact("daemon", ARTIFACT_BYTES);

  assert_eq!(
    tokio::time::timeout(
      DRIVE_DEADLINE,
      rig
        .session
        .ota()
        .push_daemon(rig.device_id(), Arc::new(FileSource::open(artifact.clone())), None)
    )
    .await
    .expect("the first drive ended"),
    OtaPhaseSnapshot::Completed
  );
  let first_acks = rig.said().acked.len();

  assert_eq!(
    tokio::time::timeout(
      DRIVE_DEADLINE,
      rig
        .session
        .ota()
        .push_daemon(rig.device_id(), Arc::new(FileSource::open(artifact.clone())), None)
    )
    .await
    .expect("the second drive ended"),
    OtaPhaseSnapshot::Completed,
    "the same artifact pushed twice is not a stuck update"
  );

  let said = rig.said();
  assert!(
    said.acked.len() > first_acks,
    "the second drive is a real transfer of its own, not a replay of the first"
  );
  assert_eq!(
    said.acked.last().copied(),
    Some(ARTIFACT_BYTES as u32),
    "and it also ends on the whole artifact"
  );
}

const TRANSCRIPT_SCENARIO: &[&str] = &[
  "the link comes up and both sides announce",
  "a provider attaches and publishes a track the daemon adopts",
  "every inbound request surface is routed once, in probe order, each driven to its reply",
];

const TRANSCRIPT_NORMALIZED: &[&str] = &[
  "request ids are sequence numbers in first-seen order; a reply carries the number of the request it answers",
  "commands and events carry no id at all: theirs is a fresh uuid per run and correlates nothing",
  "payloads are not recorded, so no uuid, clock reading or daemon-side identifier enters the fixture",
  "within a direction nothing is reordered: every step is driven to its wire effect before the next \
   one starts, and the two concurrent request arms (asset.request, system.otaAssetRange) are awaited \
   on the wire like the inline ones",
  "the two directions are recorded as two lists rather than one interleaving: a lane is ordered, but \
   nothing orders a frame the daemon composed against one the session composed at the same moment, \
   and the daemon's ancs echo of an announce lands on either side of the connect sequence's \
   time.snapshot depending on how loaded the box is",
  "system.deviceNicknameChanged is dropped: the daemon's nickname observer broadcasts once when it \
   spawns, to whoever is connected at that instant, so whether the rig's link exists yet is a race \
   between daemon bring-up and the first gateway connection and has nothing to do with the session",
  "notifications.ancsAuthStateChanged is dropped: the daemon echoes one per capabilities announce, \
   composed on its own task, so its place among the replies the daemon writes at the same moment \
   (the connect-time webapp shelf fetch) is scheduler order, not protocol order",
];

const DROPPED: &[&str] = &["system.deviceNicknameChanged", "notifications.ancsAuthStateChanged"];

#[derive(Debug, Serialize)]
struct TranscriptSnapshot {
  scenario: Vec<String>,
  normalized: Vec<String>,
  #[serde(rename = "toHost")]
  to_host: Vec<WireEntry>,
  #[serde(rename = "toDevice")]
  to_device: Vec<WireEntry>,
}

fn transcript_fixture() -> PathBuf {
  PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/session-transcript.snap.json")
}

async fn scripted_session() -> Vec<WireEntry> {
  let rig = Rig::recording().await;

  assert!(
    rig
      .await_event(SETTLE, |event| matches!(event, SessionEvent::PeerConnected { .. }))
      .await,
    "the link came up"
  );
  assert!(
    rig
      .harness
      .wait_for(
        |state| state
          .capabilities
          .snapshot()
          .gateway
          .is_some_and(|info| info.app_name == "rig"),
        SETTLE,
      )
      .await,
    "the daemon adopted the announce"
  );

  let source: Arc<FakeSource> = FakeSource::new("rig");
  rig
    .session
    .add_provider(source.clone() as Arc<dyn Provider>)
    .await
    .expect("the provider attaches");
  source.submit(playing("the transcript track"));
  assert!(
    rig
      .harness
      .wait_for(
        |state| state
          .player
          .state_reply()
          .state
          .track
          .and_then(|track| track.title)
          .as_deref()
          == Some("the transcript track"),
        SETTLE,
      )
      .await,
    "the daemon adopted the track"
  );

  let peer = rig.session.peer_for(rig.device_id()).expect("the link is up");
  let gateway = rig.gateway();
  for probe in support::probes() {
    let want = rig.replies() + 1;
    route(&peer, probe.msg, gateway.connection())
      .await
      .expect("the routing path accepted the request");
    assert!(
      rig.await_replies(SETTLE, want).await,
      "{} never put its reply on the wire",
      probe.name
    );
  }

  rig
    .transcript()
    .into_iter()
    .filter(|entry| !DROPPED.contains(&entry.msg.as_str()))
    .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn the_scripted_session_puts_the_same_frames_on_the_wire_every_time() {
  let first = scripted_session().await;
  let second = scripted_session().await;

  let render = |frames: &[WireEntry]| {
    let of = |dir: &str| frames.iter().filter(|entry| entry.dir == dir).cloned().collect();
    serde_json::to_string_pretty(&TranscriptSnapshot {
      scenario: TRANSCRIPT_SCENARIO.iter().map(|line| (*line).to_owned()).collect(),
      normalized: TRANSCRIPT_NORMALIZED.iter().map(|line| (*line).to_owned()).collect(),
      to_host: of(support::TO_HOST),
      to_device: of(support::TO_DEVICE),
    })
    .expect("the transcript renders")
      + "\n"
  };

  let rendered = render(&first);
  assert_eq!(
    rendered,
    render(&second),
    "two runs of one script disagreed on what crossed the link, which is a race in the assembly, \
     not a fixture to be relaxed"
  );

  let fixture = transcript_fixture();
  if std::env::var("UPDATE_TRANSCRIPT").is_ok() {
    std::fs::create_dir_all(fixture.parent().expect("the fixture has a directory")).expect("the fixture dir exists");
    std::fs::write(&fixture, &rendered).expect("the fixture writes");
    return;
  }

  let held = std::fs::read_to_string(&fixture).expect("the committed transcript exists; UPDATE_TRANSCRIPT=1 writes it");
  assert_eq!(
    held, rendered,
    "the session's wire conversation moved; re-read the diff before running with UPDATE_TRANSCRIPT=1"
  );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_snapshot_is_the_authority_and_survives_the_events_that_hinted_at_it() {
  let rig = Rig::start().await;
  rig.settle().await;

  let snapshot = rig.session.snapshot().await;
  assert_eq!(snapshot.peers.len(), 1, "the live link is a peer");
  assert_eq!(snapshot.peers[0].status, PeerLinkStatus::Connected);
  assert_eq!(snapshot.host_info.app_name, "rig");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_rename_the_device_confirms_comes_back_as_fresh_device_meta() {
  let rig = Rig::start().await;
  rig.settle().await;

  assert!(
    rig
      .await_event(SETTLE, |event| matches!(event, SessionEvent::DeviceMetaChanged { .. }))
      .await,
    "the daemon's announce seeded the first meta"
  );

  rig
    .gateway()
    .system()
    .device_set_nickname(libbridgething::gateway::DeviceSetNickname {
      nickname: "garage thing".into(),
    })
    .await
    .expect("the daemon accepts the rename");

  assert!(
    rig
      .await_event(SETTLE, |event| matches!(
        event,
        SessionEvent::DeviceMetaChanged { meta, .. } if meta.nickname.as_deref() == Some("garage thing")
      ))
      .await,
    "the broadcast the daemon answers with lands back on the host as device meta, or every screen \
     keeps showing the old name"
  );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_devices_webapp_shelf_arrives_with_the_connection() {
  let rig = Rig::start().await;
  rig.settle().await;

  assert!(
    rig
      .await_event(SETTLE, |event| matches!(event, SessionEvent::WebappsChanged { .. }))
      .await,
    "connecting fetches the device's installed webapps; without the seed the shelf only ever \
     hears install deltas and a device with apps reads as empty"
  );
}
