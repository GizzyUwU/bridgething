use std::{path::Path, time::Duration};

use bridgething_dsp::geometry::CHANNELS;
use serde_json::json;
use tokio::sync::mpsc;

use crate::{
  TAGS,
  capture::{CaptureHandle, Chunk},
  drive,
  input::{Command, MarkKind},
  session::{BYTES_PER_SEC, FRAMES_PER_SEGMENT, Session, SessionMeta, wall_unix},
  status::{SAMPLE_RATE_HZ, Shared, Stage},
  wake::WakeEvent,
};

const RESERVE_BYTES: u64 = 1024 * 1024 * 1024;
const SYNC_INTERVAL: Duration = Duration::from_secs(1);
const DRIVE_RETRY: Duration = Duration::from_secs(3);
const MAX_CONSECUTIVE_FAULTS: u32 = 3;

pub struct Recorder {
  shared: Shared,
  capture: CaptureHandle,
  chunks: mpsc::Receiver<Chunk>,
  wake: mpsc::Receiver<WakeEvent>,
  commands: mpsc::Receiver<Command>,
  session: Option<Session>,
  wanted: bool,
  faults: u32,
  seen_overruns: u64,
  tag: usize,
  meta: SessionMeta,
}

impl Recorder {
  pub fn new(
    shared: Shared,
    capture: CaptureHandle,
    chunks: mpsc::Receiver<Chunk>,
    wake: mpsc::Receiver<WakeEvent>,
    commands: mpsc::Receiver<Command>,
    meta: SessionMeta,
  ) -> Self {
    Self {
      shared,
      capture,
      chunks,
      wake,
      commands,
      session: None,
      wanted: true,
      faults: 0,
      seen_overruns: 0,
      tag: 0,
      meta,
    }
  }

  pub async fn run(mut self) {
    let mut sync = tokio::time::interval(SYNC_INTERVAL);
    let mut retry = tokio::time::interval(DRIVE_RETRY);
    loop {
      tokio::select! {
        Some(chunk) = self.chunks.recv() => self.on_chunk(chunk),
        Some(event) = self.wake.recv() => self.on_wake(event),
        Some(command) = self.commands.recv() => self.on_command(command),
        _ = sync.tick() => self.on_sync(),
        _ = retry.tick() => self.on_retry(),
        else => break,
      }
    }
    if let Some(session) = self.session.take() {
      let _ = session.close("daemon exiting");
    }
  }

  fn on_chunk(&mut self, chunk: Chunk) {
    if let Some(front) = chunk.front {
      self.shared.update(|status| front.apply(&mut status.telemetry));
    }
    let Some(session) = self.session.as_mut() else {
      return;
    };
    if let Err(err) = session.write_audio(&chunk.raw, &chunk.beam) {
      self.fault(format!("writing audio: {err}"));
    }
  }

  fn on_wake(&mut self, event: WakeEvent) {
    match event {
      WakeEvent::Score(score) => self.shared.update(|status| status.telemetry.wake_score = score),
      WakeEvent::Detection { score, at_sample } => {
        self.capture.mark_target();
        self.shared.update(|status| {
          status.counts.detections += 1;
          status.telemetry.wake_score = score;
        });
        self.journal("detection", json!({ "score": score, "atBeamSample": at_sample }));
      }
    }
  }

  fn on_command(&mut self, command: Command) {
    match command {
      Command::Mark(kind) => self.mark(kind),
      Command::CycleTag => {
        self.tag = (self.tag + 1) % TAGS.len();
        let tag = TAGS[self.tag];
        self.shared.update(|status| status.tag = tag.into());
        self.journal("tag", json!({ "tag": tag }));
      }
      Command::StartSession => self.start(),
      Command::StopSession => self.stop("operator"),
    }
  }

  fn mark(&mut self, kind: MarkKind) {
    self.shared.update(|status| match kind {
      MarkKind::Utterance => status.counts.marks += 1,
      MarkKind::FalseAlarm => status.counts.false_alarms += 1,
      MarkKind::Miss => status.counts.misses += 1,
    });
    if matches!(kind, MarkKind::Utterance) {
      self.capture.mark_target();
    }
    let tag = TAGS[self.tag];
    self.journal("mark", json!({ "kind": kind, "tag": tag }));
    if let Some(session) = self.session.as_mut()
      && let Err(err) = session.sync()
    {
      self.fault(format!("syncing after a mark: {err}"));
    }
  }

  fn on_sync(&mut self) {
    let overruns = self.capture.overruns();
    if overruns > self.seen_overruns {
      let lost = overruns - self.seen_overruns;
      self.seen_overruns = overruns;
      self.shared.update(|status| status.counts.dropped_chunks = overruns);
      self.journal("overrun", json!({ "chunks": lost, "total": overruns }));
      self
        .shared
        .alert(format!("{lost} audio chunk(s) dropped - the recording has a hole"));
    }

    let Some(session) = self.session.as_mut() else {
      return;
    };
    let recorded = session.recorded_secs();
    if let Err(err) = session.sync() {
      self.fault(format!("syncing: {err}"));
      return;
    }

    let disk = drive::free_space(Path::new(drive::MOUNT_POINT), BYTES_PER_SEC);
    self.shared.update(|status| {
      status.recorded_secs = recorded;
      status.disk = disk;
    });
    if disk.free_bytes < RESERVE_BYTES {
      self.stop("drive full");
      self
        .shared
        .alert("drive is full - recording stopped with the session intact");
    }
  }

  fn on_retry(&mut self) {
    if self.session.is_some() || !self.wanted {
      return;
    }
    match drive::mount_first_ext4() {
      Ok(chosen) => {
        self.shared.set_stage(Stage::Mounting {
          device: chosen.node.display().to_string(),
        });
        self.open_session(&chosen.node.display().to_string());
      }
      Err(drive::DriveError::NoDevice { looked_at }) => {
        let stage = Stage::NoDrive { looked_at };
        if self.shared.snapshot().stage != stage {
          self.shared.set_stage(stage);
        }
      }
      Err(err) => self.shared.set_stage(Stage::DriveUnusable {
        device: drive::MOUNT_POINT.into(),
        why: err.to_string(),
      }),
    }
  }

  fn open_session(&mut self, device: &str) {
    let mut meta = self.meta.clone();
    meta.started_wall_unix = wall_unix();
    match Session::open(Path::new(drive::MOUNT_POINT), meta) {
      Ok(session) => {
        self.faults = 0;
        let disk = drive::free_space(Path::new(drive::MOUNT_POINT), BYTES_PER_SEC);
        self.shared.update(|status| {
          status.recorded_secs = 0;
          status.disk = disk;
        });
        self.shared.set_stage(Stage::Recording {
          device: device.into(),
          session: session.name.clone(),
        });
        if disk.free_bytes < RESERVE_BYTES {
          self
            .shared
            .alert("drive has under a gigabyte free - this session will be short");
        }
        self.session = Some(session);
      }
      Err(err) => self.shared.set_stage(Stage::DriveUnusable {
        device: device.into(),
        why: format!("could not open a session directory: {err}"),
      }),
    }
  }

  fn stop(&mut self, why: &str) {
    self.wanted = false;
    match self.session.take() {
      Some(session) => {
        let name = session.name.clone();
        match session.close(why) {
          Ok(()) => self.shared.set_stage(Stage::Stopped {
            session: Some(name),
            why: why.into(),
          }),
          Err(err) => self.shared.set_stage(Stage::Faulted {
            session: Some(name),
            what: format!("closing the session: {err}"),
          }),
        }
      }
      None => self.shared.set_stage(Stage::Stopped {
        session: None,
        why: why.into(),
      }),
    }
    drive::unmount();
  }

  fn start(&mut self) {
    if self.session.is_some() {
      return;
    }
    self.wanted = true;
    self.faults = 0;
    self.shared.set_stage(Stage::Starting);
    self.on_retry();
  }

  fn journal(&mut self, kind: &str, body: serde_json::Value) {
    let Some(session) = self.session.as_mut() else {
      return;
    };
    if let Err(err) = session.record(kind, body) {
      self.fault(format!("writing the journal: {err}"));
    }
  }

  fn fault(&mut self, what: String) {
    let session = self.session.take().map(|session| {
      let name = session.name.clone();
      let _ = session.close("faulted");
      name
    });
    drive::unmount();
    self.faults += 1;
    self.shared.alert(what.clone());
    if self.faults >= MAX_CONSECUTIVE_FAULTS {
      self.wanted = false;
      self.shared.alert(format!(
        "gave up after {} failed sessions - fix the drive and press START",
        self.faults
      ));
    }
    self.shared.set_stage(Stage::Faulted { session, what });
  }
}

pub fn session_meta(model: Option<String>, threshold: f32, dsp: bridgething_dsp::pipeline::Config) -> SessionMeta {
  SessionMeta {
    session: String::new(),
    sample_rate_hz: SAMPLE_RATE_HZ,
    raw_channels: CHANNELS,
    raw_format: "s32le",
    beam_format: "s16le",
    frames_per_segment: FRAMES_PER_SEGMENT,
    wakeword_model: model,
    wakeword_threshold: threshold,
    steering_deg: dsp.steering_deg,
    adaptation: dsp.adaptation.is_some(),
    started_wall_unix: None,
    kernel: std::fs::read_to_string("/proc/version")
      .map(|version| version.trim().to_string())
      .unwrap_or_else(|_| "unknown".into()),
    image: std::fs::read_to_string("/usr/share/superbird/meta.json")
      .ok()
      .map(|meta| meta.trim().to_string()),
    notes: "raw is the untouched array and beam is what the wake word scored. segment n of either stream starts at sample n * framesPerSegment; segments are preallocated, so a zero tail past the last journal offset is a power cut, not audio."
      .into(),
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn stopping_without_a_session_still_says_it_is_no_longer_looking() {
    let stage = Stage::Stopped {
      session: None,
      why: "operator".into(),
    };
    assert!(
      stage.detail().contains("not looking for a drive"),
      "an idle recorder must not leave the screen reading as a search in progress"
    );
  }

  #[test]
  fn the_reserve_outlasts_a_sync_interval_by_a_wide_margin() {
    assert!(
      RESERVE_BYTES > BYTES_PER_SEC * 60,
      "the reserve has to cover more than one flush or a full drive still ends in ENOSPC"
    );
  }

  #[test]
  fn the_recorded_byte_rate_matches_the_two_streams() {
    assert_eq!(BYTES_PER_SEC, 16_000 * (4 * 4 + 2));
  }

  #[test]
  fn session_meta_records_what_the_front_end_was_configured_as() {
    let dsp = bridgething_dsp::pipeline::Config {
      adaptation: Some(bridgething_dsp::scene::Config::default()),
      ..bridgething_dsp::pipeline::Config::default()
    };
    let meta = session_meta(Some("hey_bridgething.btww".into()), 0.35, dsp);
    assert!(meta.adaptation);
    assert_eq!(meta.steering_deg, dsp.steering_deg);
    assert_eq!(meta.wakeword_threshold, 0.35);
    assert_eq!(meta.raw_channels, CHANNELS);
  }
}
