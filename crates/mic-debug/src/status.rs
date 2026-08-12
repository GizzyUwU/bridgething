use std::sync::{Arc, RwLock};

use bridgething_dsp::geometry::CHANNELS;
use serde::Serialize;
use tokio::sync::watch;

pub const SAMPLE_RATE_HZ: u32 = 16_000;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "stage", rename_all = "camelCase")]
pub enum Stage {
  Starting,
  NoDrive { looked_at: Vec<String> },
  DriveUnusable { device: String, why: String },
  Mounting { device: String },
  Recording { device: String, session: String },
  Stopped { session: Option<String>, why: String },
  Faulted { session: Option<String>, what: String },
}

impl Stage {
  pub fn headline(&self) -> String {
    match self {
      Self::Starting => "starting up".into(),
      Self::NoDrive { .. } => "NO USB DRIVE".into(),
      Self::DriveUnusable { .. } => "DRIVE UNUSABLE".into(),
      Self::Mounting { .. } => "mounting drive".into(),
      Self::Recording { .. } => "RECORDING".into(),
      Self::Stopped { .. } => "STOPPED".into(),
      Self::Faulted { .. } => "RECORDING FAILED".into(),
    }
  }

  pub fn detail(&self) -> String {
    match self {
      Self::Starting => "bringing up the microphone and the usb port".into(),
      Self::NoDrive { looked_at } if looked_at.is_empty() => {
        "nothing enumerated on the usb port. check the powered hub and the cable.".into()
      }
      Self::NoDrive { looked_at } => format!(
        "found {} but no ext4 partition to record to. reformat the drive as ext4.",
        looked_at.join(", ")
      ),
      Self::DriveUnusable { device, why } => format!("{device}: {why}"),
      Self::Mounting { device } => format!("mounting {device}"),
      Self::Recording { device, session } => format!("{session} on {device}"),
      Self::Stopped { session, why } => match session {
        Some(session) => format!("{session} closed cleanly: {why}"),
        None => format!("not recording and not looking for a drive: {why}"),
      },
      Self::Faulted { session, what } => match session {
        Some(session) => format!("{session}: {what}"),
        None => what.clone(),
      },
    }
  }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Telemetry {
  pub channel_dbfs: [f32; CHANNELS],
  pub beam_dbfs: f32,
  pub bearing_deg: Option<f64>,
  pub steering_deg: f64,
  pub point_like: bool,
  pub adopted_bins: usize,
  pub noise_measured: bool,
  pub wake_score: f32,
}

impl Default for Telemetry {
  fn default() -> Self {
    Self {
      channel_dbfs: [SILENCE_DBFS; CHANNELS],
      beam_dbfs: SILENCE_DBFS,
      bearing_deg: None,
      steering_deg: 0.0,
      point_like: false,
      adopted_bins: 0,
      noise_measured: false,
      wake_score: 0.0,
    }
  }
}

pub const SILENCE_DBFS: f32 = -120.0;

const DEAD_CHANNEL_MARGIN_DB: f32 = 30.0;

impl Telemetry {
  pub fn dead_channels(&self) -> Vec<usize> {
    let loudest = self.channel_dbfs.iter().copied().fold(SILENCE_DBFS, f32::max);
    if loudest <= SILENCE_DBFS + 6.0 {
      return Vec::new();
    }
    (0..CHANNELS)
      .filter(|&ch| self.channel_dbfs[ch] < loudest - DEAD_CHANNEL_MARGIN_DB)
      .collect()
  }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Counts {
  pub marks: u64,
  pub false_alarms: u64,
  pub misses: u64,
  pub detections: u64,
  pub dropped_chunks: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Disk {
  pub free_bytes: u64,
  pub total_bytes: u64,
  pub remaining_secs: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
  pub stage: Stage,
  pub headline: String,
  pub detail: String,
  pub recorded_secs: u64,
  pub telemetry: Telemetry,
  pub counts: Counts,
  pub disk: Disk,
  pub usb_role: String,
  pub tag: String,
  pub wakeword_loaded: bool,
  pub wakeword_threshold: f32,
  pub mic_open: bool,
  pub dead_channels: Vec<usize>,
  pub stop_hold: f32,
  pub alerts: Vec<String>,
}

impl Default for Status {
  fn default() -> Self {
    Self {
      stage: Stage::Starting,
      headline: Stage::Starting.headline(),
      detail: Stage::Starting.detail(),
      recorded_secs: 0,
      telemetry: Telemetry::default(),
      counts: Counts::default(),
      disk: Disk::default(),
      usb_role: "unknown".into(),
      tag: crate::TAGS[0].into(),
      wakeword_loaded: false,
      wakeword_threshold: crate::WAKEWORD_THRESHOLD,
      mic_open: false,
      dead_channels: Vec::new(),
      stop_hold: 0.0,
      alerts: Vec::new(),
    }
  }
}

const MAX_ALERTS: usize = 6;

#[derive(Clone)]
pub struct Shared {
  inner: Arc<RwLock<Status>>,
  tx: watch::Sender<u64>,
}

impl Shared {
  pub fn new() -> Self {
    let (tx, _) = watch::channel(0);
    Self {
      inner: Arc::new(RwLock::new(Status::default())),
      tx,
    }
  }

  pub fn snapshot(&self) -> Status {
    self.inner.read().expect("status lock poisoned").clone()
  }

  pub fn subscribe(&self) -> watch::Receiver<u64> {
    self.tx.subscribe()
  }

  pub fn update(&self, edit: impl FnOnce(&mut Status)) {
    {
      let mut guard = self.inner.write().expect("status lock poisoned");
      edit(&mut guard);
      guard.dead_channels = guard.telemetry.dead_channels();
      guard.headline = guard.stage.headline();
      guard.detail = guard.stage.detail();
    }
    self.tx.send_modify(|version| *version = version.wrapping_add(1));
  }

  pub fn set_stage(&self, stage: Stage) {
    tracing::info!(headline = stage.headline(), detail = stage.detail(), "stage");
    self.update(|status| status.stage = stage);
  }

  pub fn alert(&self, message: impl Into<String>) {
    let message = message.into();
    tracing::warn!("{message}");
    self.update(|status| {
      status.alerts.insert(0, message);
      status.alerts.truncate(MAX_ALERTS);
    });
  }
}

pub fn dbfs(sum_squares: f64, count: usize) -> f32 {
  if count == 0 {
    return SILENCE_DBFS;
  }
  let rms = (sum_squares / count as f64).sqrt();
  if rms <= 0.0 {
    return SILENCE_DBFS;
  }
  (20.0 * rms.log10()).max(SILENCE_DBFS as f64) as f32
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn a_silent_array_reports_no_dead_channels() {
    let telemetry = Telemetry::default();
    assert!(
      telemetry.dead_channels().is_empty(),
      "a parked device must not accuse its own mics"
    );
  }

  #[test]
  fn one_channel_far_under_the_others_reads_as_dead() {
    let telemetry = Telemetry {
      channel_dbfs: [-30.0, -32.0, -110.0, -31.0],
      ..Telemetry::default()
    };
    assert_eq!(telemetry.dead_channels(), vec![2]);
  }

  #[test]
  fn a_quiet_but_live_array_is_not_dead() {
    let telemetry = Telemetry {
      channel_dbfs: [-58.0, -61.0, -60.0, -59.0],
      ..Telemetry::default()
    };
    assert!(telemetry.dead_channels().is_empty());
  }

  #[test]
  fn dbfs_of_full_scale_is_zero() {
    assert!((dbfs(1.0, 1) - 0.0).abs() < 1e-6);
  }

  #[test]
  fn alerts_keep_the_newest_and_drop_the_oldest() {
    let shared = Shared::new();
    for n in 0..MAX_ALERTS + 3 {
      shared.alert(format!("alert {n}"));
    }
    let status = shared.snapshot();
    assert_eq!(status.alerts.len(), MAX_ALERTS);
    assert_eq!(status.alerts[0], format!("alert {}", MAX_ALERTS + 2));
  }

  #[test]
  fn the_headline_tracks_the_stage_without_a_separate_write() {
    let shared = Shared::new();
    shared.set_stage(Stage::Recording {
      device: "/dev/sda1".into(),
      session: "session-0001".into(),
    });
    assert_eq!(shared.snapshot().headline, "RECORDING");
  }
}
