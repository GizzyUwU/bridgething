use std::{
  collections::BTreeSet,
  path::{Path, PathBuf},
};

use libbridgething::{
  Priority,
  gateway::{BridgeToGatewayMsg, GatewayToBridgeMsg},
  protocol::{BridgeEndec, GatewayEndec, PrioritizedFrame},
};
use rmpv::Value;
use serde::Deserialize;
use tokio_util::{
  bytes::BytesMut,
  codec::{Decoder, Encoder},
};

const UNKNOWN_KEY: &str = "__field_a_newer_daemon_added__";

#[derive(Debug, Deserialize)]
struct GoldenFile {
  fixtures: Vec<GoldenFixture>,
}

#[derive(Debug, Deserialize)]
struct GoldenFixture {
  name: String,
  direction: Direction,
  priority: String,
  decoded_json: serde_json::Value,
  msgpack_hex: String,
  framed_hex: String,
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Direction {
  BridgeToGateway,
  GatewayToBridge,
}

fn fixtures() -> Vec<GoldenFixture> {
  let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../lib/fixtures/golden.json");
  let raw = std::fs::read_to_string(&path).unwrap_or_else(|err| panic!("read {}: {err}", path.display()));
  let file: GoldenFile = serde_json::from_str(&raw).expect("the golden file parses");
  assert!(!file.fixtures.is_empty(), "the golden file carries fixtures");
  file.fixtures
}

fn unhex(text: &str) -> Vec<u8> {
  assert!(text.len().is_multiple_of(2), "hex is byte aligned");
  (0..text.len())
    .step_by(2)
    .map(|at| u8::from_str_radix(&text[at..at + 2], 16).expect("hex digit"))
    .collect()
}

fn priority(label: &str) -> Priority {
  match label {
    "normal" => Priority::Normal,
    "bulk" => Priority::Bulk,
    "background" => Priority::Background,
    other => panic!("unknown priority label {other}"),
  }
}

fn decode_framed(fixture: &GoldenFixture) -> (Priority, serde_json::Value) {
  let mut wire = BytesMut::from(&unhex(&fixture.framed_hex)[..]);
  match fixture.direction {
    Direction::BridgeToGateway => {
      let frame: PrioritizedFrame<BridgeToGatewayMsg> = GatewayEndec::default()
        .decode(&mut wire)
        .unwrap_or_else(|err| panic!("decode {}: {err}", fixture.name))
        .unwrap_or_else(|| panic!("{} is a complete frame", fixture.name))
        .frame()
        .unwrap_or_else(|| panic!("{} decodes to a good frame", fixture.name));
      assert!(wire.is_empty(), "{} decoded to exactly one frame", fixture.name);
      (frame.priority, serde_json::to_value(&frame.msg).expect("json"))
    }
    Direction::GatewayToBridge => {
      let frame: PrioritizedFrame<GatewayToBridgeMsg> = BridgeEndec::default()
        .decode(&mut wire)
        .unwrap_or_else(|err| panic!("decode {}: {err}", fixture.name))
        .unwrap_or_else(|| panic!("{} is a complete frame", fixture.name))
        .frame()
        .unwrap_or_else(|| panic!("{} decodes to a good frame", fixture.name));
      assert!(wire.is_empty(), "{} decoded to exactly one frame", fixture.name);
      (frame.priority, serde_json::to_value(&frame.msg).expect("json"))
    }
  }
}

#[test]
fn every_golden_frame_decodes_to_its_frozen_shape() {
  for fixture in fixtures() {
    let (lane, decoded) = decode_framed(&fixture);
    assert_eq!(
      lane,
      priority(&fixture.priority),
      "{} decoded onto the lane it was framed on",
      fixture.name
    );
    assert_eq!(
      decoded, fixture.decoded_json,
      "{} decoded to a different shape",
      fixture.name
    );
  }
}

#[test]
fn every_outbound_golden_message_re_encodes_byte_for_byte() {
  for fixture in fixtures() {
    if fixture.direction != Direction::GatewayToBridge {
      continue;
    }
    let msg: GatewayToBridgeMsg = rmp_serde::from_slice(&unhex(&fixture.msgpack_hex))
      .unwrap_or_else(|err| panic!("decode {}: {err}", fixture.name));

    let mut wire = BytesMut::new();
    GatewayEndec::default()
      .encode(PrioritizedFrame::new(priority(&fixture.priority), msg), &mut wire)
      .unwrap_or_else(|err| panic!("encode {}: {err}", fixture.name));

    assert_eq!(
      &wire[..],
      &unhex(&fixture.framed_hex)[..],
      "{} re-encoded to different bytes",
      fixture.name
    );
  }
}

fn inject_at_top(value: &mut Value) {
  if let Value::Map(entries) = value {
    entries.push((Value::from(UNKNOWN_KEY), Value::from("ignore me")));
  } else {
    panic!("a wire message is a named map");
  }
}

fn inject_everywhere(value: &mut Value) {
  match value {
    Value::Map(entries) => {
      for (_, nested) in entries.iter_mut() {
        inject_everywhere(nested);
      }
      entries.push((Value::from(UNKNOWN_KEY), Value::from("ignore me")));
    }
    Value::Array(items) => {
      for item in items.iter_mut() {
        inject_everywhere(item);
      }
    }
    _ => {}
  }
}

fn repack(fixture: &GoldenFixture, inject: impl FnOnce(&mut Value)) -> Vec<u8> {
  let mut value = rmpv::decode::read_value(&mut &unhex(&fixture.msgpack_hex)[..])
    .unwrap_or_else(|err| panic!("read {} as a msgpack value: {err}", fixture.name));
  inject(&mut value);
  let mut out = Vec::new();
  rmpv::encode::write_value(&mut out, &value).unwrap_or_else(|err| panic!("rewrite {}: {err}", fixture.name));
  out
}

fn decode_packed(fixture: &GoldenFixture, packed: &[u8]) -> Result<serde_json::Value, rmp_serde::decode::Error> {
  match fixture.direction {
    Direction::BridgeToGateway => {
      rmp_serde::from_slice::<BridgeToGatewayMsg>(packed).map(|msg| serde_json::to_value(&msg).expect("json"))
    }
    Direction::GatewayToBridge => {
      rmp_serde::from_slice::<GatewayToBridgeMsg>(packed).map(|msg| serde_json::to_value(&msg).expect("json"))
    }
  }
}

#[test]
fn an_unknown_top_level_field_decodes_and_changes_nothing() {
  for fixture in fixtures() {
    let packed = repack(&fixture, inject_at_top);
    let decoded = decode_packed(&fixture, &packed)
      .unwrap_or_else(|err| panic!("{} rejected an unknown field: {err}", fixture.name));
    assert_eq!(
      decoded, fixture.decoded_json,
      "{} decoded differently once an unknown field rode along",
      fixture.name
    );
  }
}

#[test]
fn unknown_fields_at_every_depth_still_decode() {
  for fixture in fixtures() {
    let packed = repack(&fixture, inject_everywhere);
    decode_packed(&fixture, &packed)
      .unwrap_or_else(|err| panic!("{} rejected an unknown nested field: {err}", fixture.name));
  }
}

fn rust_sources(root: &Path, into: &mut Vec<PathBuf>) {
  for entry in std::fs::read_dir(root).unwrap_or_else(|err| panic!("read {}: {err}", root.display())) {
    let path = entry.expect("a directory entry").path();
    if path.is_dir() {
      rust_sources(&path, into);
    } else if path.extension().is_some_and(|ext| ext == "rs") {
      into.push(path);
    }
  }
}

#[test]
fn no_wire_type_denies_unknown_fields() {
  let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../lib/src");
  let mut sources = Vec::new();
  rust_sources(&root, &mut sources);
  assert!(!sources.is_empty(), "the wire crate has sources at {}", root.display());

  let mut offenders = BTreeSet::new();
  for path in sources {
    let text = std::fs::read_to_string(&path).unwrap_or_else(|err| panic!("read {}: {err}", path.display()));
    for (at, line) in text.lines().enumerate() {
      if line.contains("deny_unknown_fields") {
        offenders.insert(format!("{}:{}", path.display(), at + 1));
      }
    }
  }

  assert!(
    offenders.is_empty(),
    "wire types must tolerate fields a newer peer adds, found deny_unknown_fields at {offenders:?}"
  );
}
