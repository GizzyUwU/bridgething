use std::{collections::BTreeMap, sync::Arc};

use libbridgething::{OtaKind, OtaPhase};
use uuid::Uuid;

use crate::{
  ota::event::{CANCELLED_REASON, OtaPhaseSnapshot, OtaPlanStep, OtaPollEvent},
  seam::Clock,
};

const INTERRUPTED_REASON: &str = "the device disconnected mid-update";
pub const RESUMABLE_REASON: &str = "the device disconnected mid-update; it will pick up where it left off";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OtaRunOutcome {
  Succeeded,
  Failed,
  Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OtaRunPhase {
  Idle,
  Downloading,
  Streaming,
  Verifying,
  Writing,
  Confirming,
  Reboot,
  Completed,
  Failed,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OtaRun {
  pub run_id: String,
  pub device_id: String,
  pub kind: OtaKind,
  pub phase: OtaRunPhase,
  pub steps: Vec<OtaPlanStep>,
  pub step_id: u32,
  pub started_at_ms: u64,
  pub phase_started_at_ms: u64,
  pub stage_received: Option<u64>,
  pub stage_total: Option<u64>,
  pub rate_per_sec: Option<f64>,
  pub dwl_percent: Option<u32>,
  pub outcome: Option<OtaRunOutcome>,
  pub error: Option<String>,
  pub release_version: Option<String>,
  pub daemon_version: Option<String>,
  pub image_version: Option<String>,
  pub channel: Option<String>,
  pub root_url: Option<String>,
  pub resumable: bool,
  pub webapp_id: Option<String>,
  pub webapp_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OtaResume {
  pub channel: String,
  pub root_url: String,
  pub version: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OtaAvailable {
  pub device_id: String,
  pub release_version: Option<String>,
  pub daemon_version: Option<String>,
  pub image_version: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OtaPollStatus {
  pub last_polled_at: Option<String>,
  pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum OtaStoreChange {
  Run(Box<OtaRun>),
  Available(OtaAvailable),
  Poll(OtaPollStatus),
}

pub struct OtaRunStore {
  clock: Arc<dyn Clock>,
  runs: BTreeMap<String, OtaRun>,
  available: BTreeMap<String, OtaAvailable>,
  poll: OtaPollStatus,
}

impl OtaRunStore {
  pub fn new(clock: Arc<dyn Clock>) -> Self {
    Self {
      clock,
      runs: BTreeMap::new(),
      available: BTreeMap::new(),
      poll: OtaPollStatus::default(),
    }
  }

  pub fn runs(&self) -> Vec<&OtaRun> {
    self.runs.values().collect()
  }

  pub fn run(&self, device_id: &str) -> Option<&OtaRun> {
    self.runs.get(device_id)
  }

  pub fn available(&self) -> Vec<&OtaAvailable> {
    self.available.values().collect()
  }

  pub fn poll_status(&self) -> &OtaPollStatus {
    &self.poll
  }

  pub fn open_run_kind(&self, device_id: &str) -> Option<OtaKind> {
    self
      .runs
      .get(device_id)
      .filter(|run| run.outcome.is_none())
      .map(|run| run.kind)
  }

  pub fn dismiss(&mut self, device_id: &str) -> Option<OtaRun> {
    self.runs.get(device_id)?.outcome?;
    let mut cleared = self.runs.remove(device_id)?;
    cleared.phase = OtaRunPhase::Idle;
    Some(cleared)
  }

  pub fn clear_available(&mut self, device_id: &str) -> Option<OtaStoreChange> {
    self.available.remove(device_id)?;
    Some(OtaStoreChange::Available(OtaAvailable {
      device_id: device_id.to_owned(),
      release_version: None,
      daemon_version: None,
      image_version: None,
    }))
  }

  pub fn interrupt(&mut self, device_id: &str) -> Option<OtaRun> {
    let run = self.runs.get_mut(device_id)?;
    if run.outcome.is_some() || matches!(run.phase, OtaRunPhase::Reboot | OtaRunPhase::Confirming) {
      return None;
    }
    run.resumable = resume_of(run).is_some();
    run.phase = OtaRunPhase::Failed;
    run.outcome = Some(OtaRunOutcome::Failed);
    run.error = Some(
      if run.resumable {
        RESUMABLE_REASON
      } else {
        INTERRUPTED_REASON
      }
      .to_owned(),
    );
    Some(run.clone())
  }

  pub fn take_resume(&mut self, device_id: &str) -> Option<OtaResume> {
    let run = self.runs.get_mut(device_id)?;
    if !run.resumable {
      return None;
    }
    run.resumable = false;
    resume_of(run)
  }

  pub fn note_meta(&mut self, device_id: &str, daemon_version: &str, image_version: &str) -> Option<OtaRun> {
    let run = self.runs.get(device_id)?;
    let daemon_ok = run.daemon_version.as_deref().is_none_or(|want| want == daemon_version);
    let image_ok = run.image_version.as_deref().is_none_or(|want| want == image_version);
    if !daemon_ok || !image_ok {
      return None;
    }
    let targeted = run.daemon_version.is_some() || run.image_version.is_some();
    if !targeted && run.outcome != Some(OtaRunOutcome::Succeeded) {
      return None;
    }
    let mut cleared = self.runs.remove(device_id)?;
    cleared.phase = OtaRunPhase::Idle;
    cleared.outcome = Some(OtaRunOutcome::Succeeded);
    cleared.error = None;
    Some(cleared)
  }

  pub fn annotate_webapp(
    &mut self,
    device_id: &str,
    webapp_id: Option<&str>,
    webapp_name: Option<&str>,
  ) -> Option<OtaRun> {
    let run = self.runs.get_mut(device_id)?;
    run.webapp_id = webapp_id.map(str::to_owned);
    run.webapp_name = webapp_name.map(str::to_owned);
    Some(run.clone())
  }

  pub fn ingest(&mut self, event: OtaPollEvent) -> Vec<OtaStoreChange> {
    let now = self.clock.unix_millis();

    match event {
      OtaPollEvent::ManifestPolled { updated_at } => {
        self.poll = OtaPollStatus {
          last_polled_at: Some(updated_at),
          error: None,
        };
        vec![OtaStoreChange::Poll(self.poll.clone())]
      }

      OtaPollEvent::ManifestPollFailed { reason } => {
        self.poll.error = Some(reason);
        vec![OtaStoreChange::Poll(self.poll.clone())]
      }

      OtaPollEvent::UpdateAvailable {
        device_id,
        release,
        daemon_version,
        image_version,
      } => {
        let entry = OtaAvailable {
          device_id: device_id.clone(),
          release_version: Some(release),
          daemon_version: Some(daemon_version),
          image_version: Some(image_version),
        };
        self.available.insert(device_id, entry.clone());
        vec![OtaStoreChange::Available(entry)]
      }

      OtaPollEvent::Planned {
        device_id,
        kind,
        release,
        daemon_version,
        image_version,
        channel,
        root_url,
        steps,
      } => {
        let run = OtaRun {
          run_id: Uuid::now_v7().to_string(),
          device_id: device_id.clone(),
          kind,
          phase: OtaRunPhase::Idle,
          step_id: steps.first().map_or(0, |step| step.id),
          steps,
          started_at_ms: now,
          phase_started_at_ms: now,
          stage_received: None,
          stage_total: None,
          rate_per_sec: None,
          dwl_percent: None,
          outcome: None,
          error: None,
          release_version: unset_if_empty(release),
          daemon_version: unset_if_empty(daemon_version),
          image_version: unset_if_empty(image_version),
          channel: unset_if_empty(channel),
          root_url: unset_if_empty(root_url),
          resumable: false,
          webapp_id: None,
          webapp_name: None,
        };
        self.runs.insert(device_id, run.clone());
        vec![OtaStoreChange::Run(Box::new(run))]
      }

      OtaPollEvent::Progress {
        device_id,
        step_id,
        snapshot,
        ..
      } => {
        let Some(run) = self.runs.get_mut(&device_id) else {
          return Vec::new();
        };
        let before = run.phase;
        if run.steps.is_empty() || run.steps.iter().any(|step| step.id == step_id) {
          run.step_id = step_id;
        }
        apply_snapshot(snapshot, run);
        if run.phase != before {
          run.phase_started_at_ms = now;
        }
        vec![OtaStoreChange::Run(Box::new(run.clone()))]
      }

      OtaPollEvent::Updated { device_id, version, .. } => {
        let Some(run) = self.runs.get_mut(&device_id) else {
          return Vec::new();
        };
        run.phase = OtaRunPhase::Completed;
        run.outcome = Some(OtaRunOutcome::Succeeded);
        run.error = None;
        run.stage_received = None;
        run.stage_total = None;
        run.rate_per_sec = None;
        run.dwl_percent = None;
        if run.release_version.is_none() {
          run.release_version = unset_if_empty(version);
        }
        let run = run.clone();
        self.available.remove(&device_id);

        vec![
          OtaStoreChange::Run(Box::new(run)),
          OtaStoreChange::Available(OtaAvailable {
            device_id,
            release_version: None,
            daemon_version: None,
            image_version: None,
          }),
        ]
      }

      OtaPollEvent::Failed {
        device_id,
        kind,
        reason,
      } => {
        let outcome = if reason == CANCELLED_REASON {
          OtaRunOutcome::Cancelled
        } else {
          OtaRunOutcome::Failed
        };
        let run = self.runs.entry(device_id.clone()).or_insert_with(|| OtaRun {
          run_id: Uuid::now_v7().to_string(),
          device_id,
          kind,
          phase: OtaRunPhase::Failed,
          steps: Vec::new(),
          step_id: 0,
          started_at_ms: now,
          phase_started_at_ms: now,
          stage_received: None,
          stage_total: None,
          rate_per_sec: None,
          dwl_percent: None,
          outcome: None,
          error: None,
          release_version: None,
          daemon_version: None,
          image_version: None,
          channel: None,
          root_url: None,
          resumable: false,
          webapp_id: None,
          webapp_name: None,
        });
        if run.resumable {
          return Vec::new();
        }
        run.phase = OtaRunPhase::Failed;
        run.outcome = Some(outcome);
        run.error = Some(reason);
        run.stage_received = None;
        run.stage_total = None;
        run.rate_per_sec = None;
        vec![OtaStoreChange::Run(Box::new(run.clone()))]
      }
    }
  }
}

fn resume_of(run: &OtaRun) -> Option<OtaResume> {
  Some(OtaResume {
    channel: run.channel.clone()?,
    root_url: run.root_url.clone()?,
    version: run.release_version.clone()?,
  })
}

fn unset_if_empty(value: String) -> Option<String> {
  (!value.is_empty()).then_some(value)
}

fn apply_snapshot(snapshot: OtaPhaseSnapshot, run: &mut OtaRun) {
  match snapshot {
    OtaPhaseSnapshot::Idle => run.phase = OtaRunPhase::Idle,

    OtaPhaseSnapshot::Downloading {
      received,
      total,
      rate_per_sec,
      ..
    } => {
      run.phase = OtaRunPhase::Downloading;
      run.stage_received = Some(received);
      run.stage_total = Some(total);
      run.rate_per_sec = rate_per_sec;
      run.dwl_percent = None;
    }

    OtaPhaseSnapshot::Streaming {
      sent,
      total,
      rate_per_sec,
      ..
    } => {
      run.phase = OtaRunPhase::Streaming;
      run.stage_received = Some(sent);
      run.stage_total = Some(total);
      run.rate_per_sec = rate_per_sec;
      run.dwl_percent = None;
    }

    OtaPhaseSnapshot::Applying {
      phase,
      dwl_percent,
      dwl_bytes,
      ..
    } => {
      run.phase = match phase {
        OtaPhase::Streaming => OtaRunPhase::Streaming,
        OtaPhase::Verifying => OtaRunPhase::Verifying,
        OtaPhase::Writing => OtaRunPhase::Writing,
        OtaPhase::Confirming => OtaRunPhase::Confirming,
        OtaPhase::Reboot => OtaRunPhase::Reboot,
      };
      run.dwl_percent = Some(dwl_percent);
      run.stage_received = (dwl_percent < 100 && dwl_bytes > 0).then_some(dwl_bytes);
      run.stage_total = None;
    }

    OtaPhaseSnapshot::Staged => {
      run.phase = OtaRunPhase::Writing;
      run.stage_received = None;
      run.stage_total = None;
    }

    OtaPhaseSnapshot::Completed => run.phase = OtaRunPhase::Completed,

    OtaPhaseSnapshot::Failed { reason } => {
      run.phase = OtaRunPhase::Failed;
      run.error = Some(reason);
    }
  }
}
