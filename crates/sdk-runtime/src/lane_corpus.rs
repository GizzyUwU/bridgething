use std::{fs, path::PathBuf};

use libbridgething::Priority;
use serde::{Deserialize, Serialize};

const DEFAULT_MAX_EMISSION_BYTES: usize = 32768;
const DEFAULT_LANE_DEPTH: usize = 64;
const DEFAULT_STREAM: u16 = 256;
const LANE_SHARES: [f32; 3] = [0.7, 0.2, 0.1];

fn fixtures_dir() -> PathBuf {
  PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../lib/fixtures")
}

#[derive(Deserialize)]
pub struct Corpus {
  pub cases: Vec<CaseIn>,
}

#[derive(Deserialize)]
pub struct CaseIn {
  pub name: String,
  #[serde(default)]
  config: CaseConfig,
  ops: Vec<OpIn>,
}

#[derive(Deserialize, Default)]
struct CaseConfig {
  #[serde(default)]
  max_emission_bytes: Option<usize>,
  #[serde(default)]
  lane_depth: Option<usize>,
}

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum OpIn {
  Enqueue {
    id: String,
    priority: Priority,
    byte_len: usize,
    #[serde(default)]
    stream: Option<u16>,
    #[serde(default)]
    count: Option<usize>,
  },
  Drain {
    #[serde(default)]
    count: Option<usize>,
  },
  WriteComplete,
}

pub enum Op {
  Enqueue {
    id: String,
    priority: Priority,
    byte_len: usize,
    stream: u16,
  },
  Drain,
  WriteComplete,
}

impl CaseIn {
  pub fn max_emission_bytes(&self) -> usize {
    self.config.max_emission_bytes.unwrap_or(DEFAULT_MAX_EMISSION_BYTES)
  }

  pub fn lane_depth(&self) -> usize {
    self.config.lane_depth.unwrap_or(DEFAULT_LANE_DEPTH)
  }

  pub fn max_lane_bytes(&self) -> usize {
    let widest = self
      .ops
      .iter()
      .filter_map(|op| match op {
        OpIn::Enqueue { byte_len, .. } => Some(*byte_len),
        _ => None,
      })
      .max()
      .unwrap_or(1);
    self.lane_depth() * widest.max(1)
  }

  pub fn expand(&self) -> Vec<Op> {
    let mut out = Vec::new();
    for op in &self.ops {
      match op {
        OpIn::Enqueue {
          id,
          priority,
          byte_len,
          stream,
          count,
        } => {
          let stream = stream.unwrap_or(DEFAULT_STREAM);
          match count {
            Some(n) => out.extend((0..*n).map(|i| Op::Enqueue {
              id: format!("{id}{i}"),
              priority: *priority,
              byte_len: *byte_len,
              stream,
            })),
            None => out.push(Op::Enqueue {
              id: id.clone(),
              priority: *priority,
              byte_len: *byte_len,
              stream,
            }),
          }
        }
        OpIn::Drain { count } => out.extend((0..count.unwrap_or(1)).map(|_| Op::Drain)),
        OpIn::WriteComplete => out.push(Op::WriteComplete),
      }
    }
    out
  }
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Clone)]
pub struct Segment {
  pub id: String,
  pub bytes: u64,
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct Constants {
  pub lane_shares: Option<[f32; 3]>,
  pub emission_ceiling_bytes: Option<u64>,
  pub honors_configured_ceiling: bool,
  pub normal_lane_depth: Option<u64>,
  pub bulk_lane_depth: Option<u64>,
  pub background_lane_depth: Option<u64>,
  pub cross_lane_queue_cap: Option<u64>,
  pub starvation_guard_skips: Option<u64>,
  pub high_water_bytes: Option<u64>,
  pub hard_cap_bytes: Option<u64>,
  pub fragments_frames: bool,
  pub coalesces_frames: bool,
  pub priority_read_from_frame_byte: bool,
  pub min_frame_bytes_for_priority: Option<u64>,
  pub enqueue_couples_to_send: bool,
}

pub fn constants() -> Constants {
  Constants {
    lane_shares: Some(LANE_SHARES),
    emission_ceiling_bytes: Some(DEFAULT_MAX_EMISSION_BYTES as u64),
    honors_configured_ceiling: true,
    normal_lane_depth: Some(DEFAULT_LANE_DEPTH as u64),
    bulk_lane_depth: Some(DEFAULT_LANE_DEPTH as u64),
    background_lane_depth: Some(DEFAULT_LANE_DEPTH as u64),
    cross_lane_queue_cap: None,
    starvation_guard_skips: None,
    high_water_bytes: None,
    hard_cap_bytes: None,
    fragments_frames: false,
    coalesces_frames: true,
    priority_read_from_frame_byte: false,
    min_frame_bytes_for_priority: None,
    enqueue_couples_to_send: false,
  }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct EmittedStep {
  pub op: String,
  pub segments: Vec<Segment>,
  pub outcome: Option<String>,
  pub dropped_ids: Vec<String>,
  pub parked_ids: Vec<String>,
  pub queued_bytes: Option<u64>,
  pub link_dropped: bool,
}

impl EmittedStep {
  pub fn new(op: &str) -> Self {
    Self {
      op: op.to_string(),
      segments: Vec::new(),
      outcome: None,
      dropped_ids: Vec::new(),
      parked_ids: Vec::new(),
      queued_bytes: None,
      link_dropped: false,
    }
  }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct EmittedCase {
  pub name: String,
  pub steps: Vec<EmittedStep>,
}

#[derive(Serialize)]
pub struct Emitted {
  #[serde(rename = "impl")]
  pub implementation: &'static str,
  pub constants: Constants,
  pub cases: Vec<EmittedCase>,
}

#[derive(Deserialize)]
struct Expectation {
  constants: Constants,
  cases: Vec<EmittedCase>,
}

pub enum Emission {
  Exact,
  Fragmented,
}

pub fn corpus() -> Corpus {
  serde_json::from_str(&fs::read_to_string(fixtures_dir().join("lane-trace.json")).expect("corpus readable"))
    .expect("corpus parses")
}

pub fn write_trace(emitted: &Emitted) {
  fs::write(
    fixtures_dir().join(format!("lane-trace.{}.json", emitted.implementation)),
    format!("{}\n", serde_json::to_string_pretty(emitted).expect("serializes")),
  )
  .expect("trace written");
}

fn scheduler_order(steps: &[EmittedStep]) -> Vec<Segment> {
  let mut out: Vec<Segment> = Vec::new();
  for segment in steps.iter().flat_map(|step| &step.segments) {
    match out.last_mut() {
      Some(last) if last.id == segment.id => last.bytes += segment.bytes,
      _ => out.push(segment.clone()),
    }
  }
  out
}

pub fn assert_conforms(emitted: &Emitted, emission: Emission) {
  let expected: Expectation = serde_json::from_str(
    &fs::read_to_string(fixtures_dir().join("lane-trace.expected.json")).expect("expectation readable"),
  )
  .expect("expectation parses");

  let arm = emitted.implementation;
  let fragmented = matches!(emission, Emission::Fragmented);
  let mut want_constants = expected.constants;
  want_constants.fragments_frames = fragmented;
  assert_eq!(emitted.constants, want_constants, "{arm}: constants");
  assert_eq!(emitted.cases.len(), expected.cases.len(), "{arm}: case count");

  for (got, want) in emitted.cases.iter().zip(&expected.cases) {
    assert_eq!(got.name, want.name, "{arm}: case order");
    assert_eq!(got.steps.len(), want.steps.len(), "{arm}: {} step count", want.name);
    for (index, (step, expect)) in got.steps.iter().zip(&want.steps).enumerate() {
      let at = format!("{arm}: {} step {index} ({})", want.name, expect.op);
      assert_eq!(step.op, expect.op, "{at}: op");
      assert_eq!(step.outcome, expect.outcome, "{at}: outcome");
      assert_eq!(step.dropped_ids, expect.dropped_ids, "{at}: dropped_ids");
      assert_eq!(step.link_dropped, expect.link_dropped, "{at}: link_dropped");
      if !fragmented {
        assert_eq!(step.segments, expect.segments, "{at}: segments");
      }
    }
    if fragmented {
      assert_eq!(
        scheduler_order(&got.steps),
        scheduler_order(&want.steps),
        "{arm}: {} scheduler order",
        want.name
      );
    }
  }
}
