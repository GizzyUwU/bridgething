//! Cross-language wire-format golden vectors.
//!
//! Canonical bridgething wire messages constructed in Rust, encoded as
//! `framed_hex` (16-byte header + msgpack-named, compression=NONE for
//! determinism) plus the equivalent `decoded_json` shape. Every language
//! in the polyglot SDK (Swift, Kotlin, TS) round-trips these fixtures to
//! prove its codec agrees with Rust on the wire.
//!
//! Regenerate with: `UPDATE_GOLDEN=1 cargo test -p libbridgething --test golden`

use std::path::PathBuf;

use libbridgething::gateway::*;
use libbridgething::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

const HEADER_LEN: usize = 16;
const MAGIC: u16 = 0xdead;
const VERSION: u8 = 2;
const COMPRESSION_NONE: u8 = 0x00;
const ENCODING_MSGPACK: u8 = 0x00;

const FIXED_ID: &str = "0192f2a0-bbb0-7c00-a000-000000000001";
const FIXED_REQUEST_ID: &str = "0192f2a0-bbb0-7c00-a000-000000000099";

#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct GoldenFile {
  /// Wire-format version number (matches the codec's `VERSION` constant).
  version: u8,
  /// Hex string of the magic bytes (matches the codec's `MAGIC`).
  magic: String,
  /// Header layout, for cross-language readers that don't want to chase the spec.
  header: HeaderSpec,
  fixtures: Vec<GoldenFixture>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct HeaderSpec {
  size_bytes: usize,
  fields: Vec<HeaderField>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct HeaderField {
  name: String,
  offset: usize,
  size: usize,
  description: String,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct GoldenFixture {
  /// Stable identifier; tests reference fixtures by this name.
  name: String,
  /// Plain-language description for humans reading the JSON.
  description: String,
  /// Which direction (and therefore which Rust type) this message decodes to.
  direction: Direction,
  /// Canonical structural form of the decoded message. Cross-language tests
  /// should compare structurally, not by string match — field order from
  /// msgpack named maps is implementation-defined.
  decoded_json: serde_json::Value,
  /// Hex string of `rmp_serde::to_vec_named(&msg)`. The framed payload, pre-
  /// compression. Useful for testing the msgpack layer in isolation.
  msgpack_hex: String,
  /// Hex string of the full wire frame: 16-byte header (compression=0,
  /// encoding=0) followed by `msgpack_hex` bytes.
  framed_hex: String,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Copy)]
#[serde(rename_all = "snake_case")]
enum Direction {
  BridgeToGateway,
  GatewayToBridge,
}

fn fixture_path() -> PathBuf {
  PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/golden.json")
}

fn header_spec() -> HeaderSpec {
  HeaderSpec {
    size_bytes: HEADER_LEN,
    fields: vec![
      HeaderField {
        name: "magic".into(),
        offset: 0,
        size: 2,
        description: "u16 BE, always 0xdead".into(),
      },
      HeaderField {
        name: "version".into(),
        offset: 2,
        size: 1,
        description: "u8 wire-format version".into(),
      },
      HeaderField {
        name: "compression".into(),
        offset: 3,
        size: 1,
        description: "u8: 0x00 none, 0x01 gzip".into(),
      },
      HeaderField {
        name: "encoding".into(),
        offset: 4,
        size: 1,
        description: "u8: 0x00 msgpack-named, 0x01 json".into(),
      },
      HeaderField {
        name: "reserved".into(),
        offset: 5,
        size: 3,
        description: "must be zero on encode, ignored on decode".into(),
      },
      HeaderField {
        name: "length".into(),
        offset: 8,
        size: 8,
        description: "u64 BE byte length of payload (post-compression)".into(),
      },
    ],
  }
}

fn frame(payload: &[u8]) -> Vec<u8> {
  let mut out = Vec::with_capacity(HEADER_LEN + payload.len());
  out.extend_from_slice(&MAGIC.to_be_bytes());
  out.push(VERSION);
  out.push(COMPRESSION_NONE);
  out.push(ENCODING_MSGPACK);
  out.extend_from_slice(&[0, 0, 0]); // reserved
  out.extend_from_slice(&(payload.len() as u64).to_be_bytes());
  out.extend_from_slice(payload);
  out
}

fn hex(bytes: &[u8]) -> String {
  let mut s = String::with_capacity(bytes.len() * 2);
  for b in bytes {
    s.push_str(&format!("{b:02x}"));
  }
  s
}

fn id() -> Uuid {
  FIXED_ID.parse().unwrap()
}

fn req_id() -> Uuid {
  FIXED_REQUEST_ID.parse().unwrap()
}

fn bridge_meta() -> BridgeThingMeta {
  BridgeThingMeta {
    bridgething_version: "0.1.0".into(),
    libbridgething_version: "v0.1.0".into(),
    app_name: "bridgething".into(),
    app_version: "0.1.0".into(),
    os_name: "linux".into(),
    os_version: "6.19".into(),
    os_description: "bridgething wrynose".into(),
    bt_mac: "aa:bb:cc:dd:ee:ff".into(),
    serial_number: "GOLDEN-0001".into(),
    fcc_id: "fcc-test".into(),
    ic_id: "ic-test".into(),
    model_name: "Car Thing".into(),
    image_build_id: "golden-build-id".into(),
    image_build_date: "2026-04-27T00:00:00Z".into(),
    image_distro: "bridgething".into(),
    image_distro_version: "v1".into(),
    image_machine: "superbird".into(),
    discord: "https://discord.example".into(),
    credits: "the car thing scene".into(),
  }
}

fn gateway_meta() -> GatewayMeta {
  GatewayMeta {
    adapter_version: "1.0.0".into(),
    lib_version: "1.0.0".into(),
    libbridgething_version: "v0.1.0".into(),
    app_name: "bridgething-mobile".into(),
    app_version: "1.0.0".into(),
    os_name: "iOS".into(),
  }
}

fn fingerprint_bytes() -> Vec<u8> {
  // PNG magic + a couple bytes — small but distinctive payload for the binary
  // path.
  vec![0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]
}

fn build_fixtures() -> Vec<(GoldenFixture, Vec<u8>)> {
  let mut out = Vec::new();

  out.push(bridge_fixture(
    "bridge_to_gateway/version-event",
    "daemon announcing its version + hardware metadata as an event",
    BridgeToGatewayMsg {
      id: id(),
      meta: GatewayMsgMeta::Event,
      data: BridgeToGatewayMsgData::Version(bridge_meta()),
    },
  ));

  out.push(bridge_fixture(
    "bridge_to_gateway/ack-response",
    "ack to a request — meta carries requestId, data is the unit Ack variant",
    BridgeToGatewayMsg {
      id: id(),
      meta: GatewayMsgMeta::Response(ResponseMeta { request_id: req_id() }),
      data: BridgeToGatewayMsgData::Ack,
    },
  ));

  out.push(bridge_fixture(
    "bridge_to_gateway/done-response",
    "completion response to a long-running command",
    BridgeToGatewayMsg {
      id: id(),
      meta: GatewayMsgMeta::Response(ResponseMeta { request_id: req_id() }),
      data: BridgeToGatewayMsgData::Done,
    },
  ));

  out.push(bridge_fixture(
    "bridge_to_gateway/file-files-response",
    "response to a fileList request — daemon returns the served file list",
    BridgeToGatewayMsg {
      id: id(),
      meta: GatewayMsgMeta::Response(ResponseMeta { request_id: req_id() }),
      data: BridgeToGatewayMsgData::File(BridgeToGatewayFileMsg::Files(FileList {
        files: vec!["index.html".into(), "app.js".into(), "style.css".into()],
      })),
    },
  ));

  out.push(bridge_fixture(
    "bridge_to_gateway/file-request-request",
    "daemon requests a file from the gateway (browser fetched something we don't have)",
    BridgeToGatewayMsg {
      id: id(),
      meta: GatewayMsgMeta::Request,
      data: BridgeToGatewayMsgData::File(BridgeToGatewayFileMsg::FileRequest(FileRequestData {
        file: "/missing/asset.png".into(),
      })),
    },
  ));

  out.push(bridge_fixture(
    "bridge_to_gateway/forward-text-event",
    "arbitrary text payload over the Forward escape hatch",
    BridgeToGatewayMsg {
      id: id(),
      meta: GatewayMsgMeta::Event,
      data: BridgeToGatewayMsgData::Forward(ForwardMessage::Text("hello, gateway".into())),
    },
  ));

  out.push(bridge_fixture(
    "bridge_to_gateway/forward-json-event",
    "arbitrary JSON payload over the Forward escape hatch",
    BridgeToGatewayMsg {
      id: id(),
      meta: GatewayMsgMeta::Event,
      data: BridgeToGatewayMsgData::Forward(ForwardMessage::Json(serde_json::json!({
        "kind": "playback-changed",
        "payload": { "playing": true, "positionMs": 12345 }
      }))),
    },
  ));

  out.push(bridge_fixture(
    "bridge_to_gateway/forward-binary-event",
    "raw bytes over the Forward escape hatch — verifies msgpack bin tag, not base64 string",
    BridgeToGatewayMsg {
      id: id(),
      meta: GatewayMsgMeta::Event,
      data: BridgeToGatewayMsgData::Forward(ForwardMessage::Binary(fingerprint_bytes())),
    },
  ));

  out.push(gateway_fixture(
    "gateway_to_bridge/version-event",
    "phone announcing its gateway version",
    GatewayToBridgeMsg {
      id: id(),
      meta: GatewayMsgMeta::Event,
      data: GatewayToBridgeMsgData::Version(gateway_meta()),
    },
  ));

  out.push(gateway_fixture(
    "gateway_to_bridge/file-list-request",
    "gateway asks for the served file list — unit variant of the file enum",
    GatewayToBridgeMsg {
      id: id(),
      meta: GatewayMsgMeta::Request,
      data: GatewayToBridgeMsgData::File(GatewayToBridgeFileMsg::List),
    },
  ));

  out.push(gateway_fixture(
    "gateway_to_bridge/file-delete-command",
    "gateway tells daemon to drop some served files",
    GatewayToBridgeMsg {
      id: id(),
      meta: GatewayMsgMeta::Command,
      data: GatewayToBridgeMsgData::File(GatewayToBridgeFileMsg::Delete(FileDelete {
        files: vec!["stale.html".into()],
      })),
    },
  ));

  out.push(gateway_fixture(
    "gateway_to_bridge/file-add-command",
    "gateway uploads a file — exercises BridgeFile (path + raw bytes)",
    GatewayToBridgeMsg {
      id: id(),
      meta: GatewayMsgMeta::Command,
      data: GatewayToBridgeMsgData::File(GatewayToBridgeFileMsg::Add(FileAdd {
        files: vec![BridgeFile {
          path: "/asset.png".into(),
          data: fingerprint_bytes(),
        }],
      })),
    },
  ));

  out.push(gateway_fixture(
    "gateway_to_bridge/file-response-response",
    "gateway answering a fileRequest from the daemon — returns BridgeFile",
    GatewayToBridgeMsg {
      id: id(),
      meta: GatewayMsgMeta::Response(ResponseMeta { request_id: req_id() }),
      data: GatewayToBridgeMsgData::File(GatewayToBridgeFileMsg::FileResponse(FileResponseData {
        file: BridgeFile {
          path: "/missing/asset.png".into(),
          data: fingerprint_bytes(),
        },
      })),
    },
  ));

  out.push(gateway_fixture(
    "gateway_to_bridge/chrome-navigate-command",
    "gateway driving the Car Thing chromium kiosk to a new URL",
    GatewayToBridgeMsg {
      id: id(),
      meta: GatewayMsgMeta::Command,
      data: GatewayToBridgeMsgData::Chrome(GatewayToBridgeChromeMsg::Navigate(ChromeNavigate {
        url: "https://example.com".into(),
      })),
    },
  ));

  out.push(gateway_fixture(
    "gateway_to_bridge/forward-binary-event",
    "gateway sending raw bytes through the Forward escape hatch",
    GatewayToBridgeMsg {
      id: id(),
      meta: GatewayMsgMeta::Event,
      data: GatewayToBridgeMsgData::Forward(ForwardMessage::Binary(fingerprint_bytes())),
    },
  ));

  out
}

fn bridge_fixture(name: &str, description: &str, msg: BridgeToGatewayMsg) -> (GoldenFixture, Vec<u8>) {
  let packed = rmp_serde::to_vec_named(&msg).expect("encode bridge msg");
  let framed = frame(&packed);
  let decoded_json = serde_json::to_value(&msg).expect("re-encode as json");
  let fix = GoldenFixture {
    name: name.into(),
    description: description.into(),
    direction: Direction::BridgeToGateway,
    decoded_json,
    msgpack_hex: hex(&packed),
    framed_hex: hex(&framed),
  };
  (fix, packed)
}

fn gateway_fixture(name: &str, description: &str, msg: GatewayToBridgeMsg) -> (GoldenFixture, Vec<u8>) {
  let packed = rmp_serde::to_vec_named(&msg).expect("encode gateway msg");
  let framed = frame(&packed);
  let decoded_json = serde_json::to_value(&msg).expect("re-encode as json");
  let fix = GoldenFixture {
    name: name.into(),
    description: description.into(),
    direction: Direction::GatewayToBridge,
    decoded_json,
    msgpack_hex: hex(&packed),
    framed_hex: hex(&framed),
  };
  (fix, packed)
}

fn current() -> GoldenFile {
  GoldenFile {
    version: VERSION,
    magic: format!("0x{:04x}", MAGIC),
    header: header_spec(),
    fixtures: build_fixtures().into_iter().map(|(f, _)| f).collect(),
  }
}

#[test]
fn golden_vectors_match_fixture_file() {
  let current = current();

  if std::env::var("UPDATE_GOLDEN").is_ok() {
    let json = serde_json::to_string_pretty(&current).expect("serialize golden file");
    std::fs::write(fixture_path(), format!("{json}\n")).expect("write fixture file");
    eprintln!("wrote {} fixtures to {}", current.fixtures.len(), fixture_path().display());
    return;
  }

  let on_disk = std::fs::read_to_string(fixture_path()).unwrap_or_else(|err| {
    panic!(
      "failed to read {}: {err}\nrun `just goldens` (or `UPDATE_GOLDEN=1 cargo test -p libbridgething --test golden`) to generate it",
      fixture_path().display()
    )
  });
  let parsed: GoldenFile = serde_json::from_str(&on_disk).expect("parse golden file");

  assert_eq!(
    parsed, current,
    "golden fixtures drifted from Rust source — run `just goldens` to regenerate"
  );
}

#[test]
fn golden_fixtures_round_trip_through_rust_codec() {
  for (fix, packed) in build_fixtures() {
    match fix.direction {
      Direction::BridgeToGateway => {
        let decoded: BridgeToGatewayMsg = rmp_serde::from_slice(&packed)
          .unwrap_or_else(|err| panic!("decode {}: {err}", fix.name));
        let re_encoded = rmp_serde::to_vec_named(&decoded)
          .unwrap_or_else(|err| panic!("re-encode {}: {err}", fix.name));
        assert_eq!(packed, re_encoded, "{} did not round-trip", fix.name);
      }
      Direction::GatewayToBridge => {
        let decoded: GatewayToBridgeMsg = rmp_serde::from_slice(&packed)
          .unwrap_or_else(|err| panic!("decode {}: {err}", fix.name));
        let re_encoded = rmp_serde::to_vec_named(&decoded)
          .unwrap_or_else(|err| panic!("re-encode {}: {err}", fix.name));
        assert_eq!(packed, re_encoded, "{} did not round-trip", fix.name);
      }
    }
  }
}
