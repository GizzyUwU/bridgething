use std::{
  fs,
  panic::{AssertUnwindSafe, catch_unwind},
  path::PathBuf,
};

use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use tokio_util::{bytes::BytesMut, codec::Decoder};

use super::{
  AUTO_GZIP_THRESHOLD_BYTES, BridgeEndec, DecodedFrame, EndecError, GatewayEndec, HEADER_LEN, MAGIC, MAX_FRAME_LEN,
  PrioritizedFrame, VERSION,
};
use crate::{
  Priority,
  gateway::{BridgeToGatewayMsg, GatewayToBridgeMsg},
};

const MAX_EVENTS_PER_CASE: usize = 64;

const NOTES: &str = "one WireEndec state machine, typed to this arm's inbound message. bad magic, bad \
  version and an over-cap length all resync to the next magic silently, with no error event, because an \
  Err out of a tokio_util Decoder ends a Framed stream for good. the length is capped while still a u64, \
  so no length field can overflow the header-plus-payload sum. unknown compression and encoding bytes \
  coerce to none/msgpack per the wire From<u8> impls, also with no error event. the parse state is taken \
  before decompression and typed decode, both of which run after the body is off the front of the buffer, \
  so either failing leaves framing intact and the following frame decodes. those two failures are reported \
  as a failed item rather than an Err, for the same reason, so a stream consumer sees them and keeps reading.";

trait ArmMsg: Serialize + DeserializeOwned {
  fn frame_id(&self) -> String;
}

impl ArmMsg for GatewayToBridgeMsg {
  fn frame_id(&self) -> String {
    self.id.to_string()
  }
}

impl ArmMsg for BridgeToGatewayMsg {
  fn frame_id(&self) -> String {
    self.id.to_string()
  }
}

#[derive(Deserialize)]
struct Corpus {
  cases: Vec<CorpusCase>,
}

#[derive(Deserialize)]
struct CorpusCase {
  name: String,
  stream_hex: String,
  chunks: Vec<usize>,
}

#[derive(Deserialize)]
struct Expectation {
  constants: serde_json::Map<String, serde_json::Value>,
  asymmetries: serde_json::Map<String, serde_json::Value>,
  cases: Vec<serde_json::Value>,
}

#[derive(Serialize)]
struct Emitted {
  #[serde(rename = "impl")]
  implementation: &'static str,
  constants: Constants,
  unobserved: Vec<&'static str>,
  notes: &'static str,
  cases: Vec<EmittedCase>,
}

#[derive(Serialize)]
struct Constants {
  header_len: usize,
  magic: u16,
  version: u8,
  max_payload_bytes: Option<usize>,
  resyncs_past_bad_magic: bool,
  rejects_unknown_compression: bool,
  rejects_unknown_encoding: bool,
  decodes_typed_payload: bool,
  auto_gzip_threshold_bytes: Option<usize>,
}

#[derive(Serialize)]
struct EmittedCase {
  name: String,
  steps: Vec<Step>,
  terminal: Terminal,
}

#[derive(Serialize)]
struct Step {
  event: &'static str,
  after_chunk: usize,
  consumed_bytes: usize,
  priority: Option<&'static str>,
  compression: Option<&'static str>,
  encoding: Option<&'static str>,
  payload_len: Option<usize>,
  payload_sha256: Option<String>,
  frame_id: Option<String>,
  error_kind: Option<&'static str>,
  error_stage: Option<&'static str>,
  recoverable: Option<bool>,
}

#[derive(Serialize)]
struct Terminal {
  state: &'static str,
  buffered_bytes: usize,
  consumed_bytes: usize,
  frames: usize,
  errors: usize,
}

fn fixtures_dir() -> PathBuf {
  PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures")
}

fn sha256_hex(bytes: &[u8]) -> String {
  let mut hasher = Sha256::new();
  hasher.update(bytes);
  hex::encode(hasher.finalize())
}

fn priority_label(priority: Priority) -> &'static str {
  match priority {
    Priority::Normal => "normal",
    Priority::Bulk => "bulk",
    Priority::Background => "background",
  }
}

fn classify(err: &EndecError) -> (&'static str, &'static str) {
  match err {
    EndecError::RmpSerialization(_) | EndecError::Io(_) | EndecError::Compression(_) => ("other", "framing"),
    EndecError::TypedDecode { .. } => ("typed_decode_failed", "typed"),
    EndecError::Decompress(_) | EndecError::DecompressTooLarge { .. } => ("decompress_failed", "decompress"),
  }
}

fn frame_step<M: ArmMsg>(after_chunk: usize, consumed_bytes: usize, frame: &PrioritizedFrame<M>) -> Step {
  let payload = rmp_serde::to_vec_named(&frame.msg).expect("re-encoding a decoded message");
  Step {
    event: "frame",
    after_chunk,
    consumed_bytes,
    priority: Some(priority_label(frame.priority)),
    compression: None,
    encoding: None,
    payload_len: Some(payload.len()),
    payload_sha256: Some(sha256_hex(&payload)),
    frame_id: Some(frame.msg.frame_id()),
    error_kind: None,
    error_stage: None,
    recoverable: None,
  }
}

fn error_step(after_chunk: usize, consumed_bytes: usize, err: &EndecError) -> Step {
  let (kind, stage) = classify(err);
  Step {
    event: "error",
    after_chunk,
    consumed_bytes,
    priority: None,
    compression: None,
    encoding: None,
    payload_len: None,
    payload_sha256: None,
    frame_id: None,
    error_kind: Some(kind),
    error_stage: Some(stage),
    recoverable: Some(err.is_recoverable()),
  }
}

fn run_case<D, M>(case: &CorpusCase) -> EmittedCase
where
  D: Decoder<Item = DecodedFrame<M>, Error = EndecError> + Default,
  M: ArmMsg,
{
  let stream = hex::decode(&case.stream_hex).expect("corpus stream_hex is valid hex");
  let mut codec = D::default();
  let mut buf = BytesMut::new();
  let mut steps: Vec<Step> = Vec::new();
  let mut fed = 0usize;
  let mut state = "ok";

  'chunks: for (chunk_idx, &chunk_len) in case.chunks.iter().enumerate() {
    buf.extend_from_slice(&stream[fed..fed + chunk_len]);
    fed += chunk_len;

    loop {
      if steps.len() >= MAX_EVENTS_PER_CASE {
        state = "stalled";
        break 'chunks;
      }

      let buffered_before = buf.len();
      let outcome = catch_unwind(AssertUnwindSafe(|| codec.decode(&mut buf)));
      let result = match outcome {
        Ok(result) => result,
        Err(_) => {
          state = "panicked";
          break 'chunks;
        }
      };

      match result {
        Ok(None) => break,
        Ok(Some(DecodedFrame::Frame(frame))) => steps.push(frame_step(chunk_idx, fed - buf.len(), &frame)),
        Ok(Some(DecodedFrame::Failed(err))) | Err(err) => {
          steps.push(error_step(chunk_idx, fed - buf.len(), &err));
          if buf.len() == buffered_before {
            state = "stalled";
            break 'chunks;
          }
        }
      }
    }
  }

  if state == "ok" && !buf.is_empty() {
    state = "incomplete";
  }

  EmittedCase {
    name: case.name.clone(),
    terminal: Terminal {
      state,
      buffered_bytes: buf.len(),
      consumed_bytes: fed - buf.len(),
      frames: steps.iter().filter(|s| s.event == "frame").count(),
      errors: steps.iter().filter(|s| s.event == "error").count(),
    },
    steps,
  }
}

fn run_corpus<D, M>(implementation: &'static str) -> Emitted
where
  D: Decoder<Item = DecodedFrame<M>, Error = EndecError> + Default,
  M: ArmMsg,
{
  let corpus: Corpus =
    serde_json::from_str(&fs::read_to_string(fixtures_dir().join("frame-stream-trace.json")).expect("corpus readable"))
      .expect("corpus parses");

  Emitted {
    implementation,
    constants: Constants {
      header_len: HEADER_LEN,
      magic: MAGIC,
      version: VERSION,
      max_payload_bytes: Some(MAX_FRAME_LEN),
      resyncs_past_bad_magic: true,
      rejects_unknown_compression: false,
      rejects_unknown_encoding: false,
      decodes_typed_payload: true,
      auto_gzip_threshold_bytes: Some(AUTO_GZIP_THRESHOLD_BYTES),
    },
    unobserved: vec!["compression", "encoding"],
    notes: NOTES,
    cases: corpus.cases.iter().map(run_case::<D, M>).collect(),
  }
}

fn emit(emitted: &Emitted) {
  fs::write(
    fixtures_dir().join(format!("frame-stream-trace.{}.json", emitted.implementation)),
    format!("{}\n", serde_json::to_string_pretty(emitted).expect("serializes")),
  )
  .expect("trace written");
}

fn assert_conforms(emitted: &Emitted) {
  let expectation: Expectation = serde_json::from_str(
    &fs::read_to_string(fixtures_dir().join("frame-stream-trace.expected.json")).expect("expectation readable"),
  )
  .expect("expectation parses");
  let got = serde_json::to_value(emitted).expect("emitted serializes");

  let constants = got["constants"].as_object().expect("constants are an object");
  let mut declared: Vec<&str> = expectation.constants.keys().map(String::as_str).collect();
  declared.sort_unstable();
  let mut present: Vec<&str> = constants.keys().map(String::as_str).collect();
  present.sort_unstable();
  assert_eq!(
    present, declared,
    "frame-stream constants moved; reconcile them into the expectation"
  );
  for (key, want) in &expectation.constants {
    assert_eq!(constants.get(key), Some(want), "constant {key}");
  }

  for field in &emitted.unobserved {
    assert!(
      expectation.asymmetries.contains_key(*field),
      "{field} is declared unobserved but the expectation does not record it as an asymmetry"
    );
  }

  let cases = got["cases"].as_array().expect("cases are an array");
  assert_eq!(cases.len(), expectation.cases.len(), "case count");
  for (got, want) in cases.iter().zip(&expectation.cases) {
    let name = want["name"].as_str().expect("expectation case has a name");
    assert_eq!(got["name"], want["name"], "case order");
    assert_eq!(got["terminal"], want["terminal"], "terminal for {name}");

    let got_steps = got["steps"].as_array().expect("steps are an array");
    let want_steps = want["steps"].as_array().expect("expected steps are an array");
    assert_eq!(got_steps.len(), want_steps.len(), "step count for {name}");
    for (index, (got_step, want_step)) in got_steps.iter().zip(want_steps).enumerate() {
      for (field, want_value) in want_step.as_object().expect("a step is an object") {
        if emitted.unobserved.contains(&field.as_str()) {
          continue;
        }
        assert_eq!(got_step.get(field), Some(want_value), "{name} step {index} {field}");
      }
    }
  }
}

#[test]
fn rust_bridge_arm_conforms_to_the_frozen_expectation() {
  let emitted = run_corpus::<BridgeEndec, GatewayToBridgeMsg>("rust-bridge");
  emit(&emitted);
  assert_conforms(&emitted);
}

#[test]
fn rust_gateway_arm_conforms_to_the_frozen_expectation() {
  let emitted = run_corpus::<GatewayEndec, BridgeToGatewayMsg>("rust-gateway");
  emit(&emitted);
  assert_conforms(&emitted);
}
