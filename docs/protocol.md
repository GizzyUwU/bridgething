# bridgething wire protocol

This document is the on-the-wire reference for both gateway-app
developers (talking to the daemon over Bluetooth) and webapp /
client-app developers (talking to the daemon over a local WebSocket).

It covers the framing layer, the message envelope every packet shares,
correlation semantics for requests, the priority lane byte, error
shapes, transports, and size / timeout limits. It does **not**
enumerate the per-surface types. Those live in the auto-generated
`crates/lib/ts/bindings/`, `crates/lib/swift/`, and
`crates/lib/kotlin/` outputs, plus the canonical Rust source under
`crates/lib/src/{client,gateway,shared}/`.

## the two wire surfaces

The daemon speaks **two** protocols that share one envelope.

| surface     | direction pair                              | endpoint                                                  | encoding | framed how                |
| ----------- | ------------------------------------------- | --------------------------------------------------------- | -------- | ------------------------- |
| **gateway** | `BridgeToGatewayMsg` ↔ `GatewayToBridgeMsg` | RFCOMM, iAP2 EA channel, or NetworkGateway WS (port 8892) | msgpack  | `BridgeEndec` byte stream |
| **client**  | `BridgeToClientMsg` ↔ `ClientToBridgeMsg`   | local WebSocket on port 8891                              | JSON     | one envelope per WS frame |

The framing layer below is for the **gateway** surface. The **client**
surface is plain JSON-per-WebSocket-text-frame; no magic byte, no
compression header, no priority byte. WebSocket framing handles
delimitation. The envelope shape (`id`, `meta`, `data`) is identical
across both surfaces, so once you've decoded an envelope you reason
about the message the same way.

The stock Spotify webapp uses a **third** WebSocket on port 8890
speaking the legacy "interapp" protocol. That is translated by the
daemon and never reaches gateways or modern webapps; ignore it unless
you're modifying the daemon itself.

## frame layout (gateway surface)

Every byte sequence the daemon and a gateway exchange over a Bluetooth
RFCOMM stream, an iAP2 EA channel, or a NetworkGateway WebSocket binary
frame is one or more concatenated **frames** in this shape:

```text
offset  size  field         notes
------  ----  ------------  -----
0       2     magic         0xdead, big-endian; resync marker
2       1     version       currently 2; mismatch drops the connection
3       1     compression   0=none (default), 1=gzip
4       1     encoding      0=msgpack (default), 1=json
5       1     priority      0=normal, 1=bulk (lane hint)
6       2     reserved      zeroed
8       8     length        big-endian u64; bytes of payload that follow
16      N     payload       msgpack (or json) of one envelope, optionally gzipped
```

Header is fixed-length 16 bytes. `length` is the size of the payload
**after** compression (i.e. the byte count actually on the wire). One
frame carries exactly one typed envelope (see [envelope](#envelope)).

A single transport message can hold multiple concatenated frames, and
a single frame can be split across multiple transport messages. The
canonical Rust decoder is `BridgeEndec` in
`crates/lib/src/protocol/bridge.rs`, implemented as a streaming
`tokio_util::codec::Decoder` for byte streams (RFCOMM, iAP2 EA), plus
`parse_bridge_frame` for the message-bounded case (a complete WS
binary frame).

### compression

Default is `none`. The codec supports `gzip` (compression byte = 1)
but bridgething turns it off by default since msgpack is already
compact and the Cortex-A53 spends more on gzip CPU than the BT link
saves on bytes for typical message sizes. If you ship gzipped frames,
the receiver decompresses transparently and types decode the same way.

### encoding

Always `msgpack` on the emit path (encoding byte = 0). The decoder
also accepts `json` (encoding byte = 1) so a gateway that prefers JSON
on the wire can be parsed, but no daemon-side caller currently emits
JSON-encoded frames. Treat `json` as decode-only support today.

The canonical msgpack encoder uses `rmp-serde to_vec_named`, which
keeps field **names** in the wire (named-map style, not array style).
This is required so polyglot decoders (Swift, Kotlin, TS) don't depend
on Rust struct field order.

### priority lane (header byte 5)

Per-frame hint for outbound batchers:

| value | meaning  | use for                                               |
| ----- | -------- | ----------------------------------------------------- |
| 0     | `normal` | small, latency-sensitive: control, state, transport   |
| 1     | `bulk`   | large or background: assets, OTA, queued chunked data |

The receiver does nothing with the byte except echo it onto its own
outbound traffic. The sender's batcher drains `normal` preferentially
and fills remaining wire space with `bulk` on every batch; `bulk`
chunks interleave between `normal` frames without any reassembly state
on the receive side, since application-layer chunking has already
broken the bulk payload into many small typed messages.

Default-zero means the byte is non-breaking against pre-priority
senders.

## envelope

Once a frame's payload is decoded (msgpack → typed object), every
message on every surface has the same outer shape:

```ts
{
  id: Uuid,           // 16 bytes; unique per outbound message
  meta: MsgMeta,      // command | event | request | response { requestId }
  data: <surface tagged enum>,
}
```

`id` is fresh on every send (the receiver never sends back the same id;
correlation goes through `meta.data.requestId` for responses, see
below). UUIDs are serialized as 16-byte arrays on the gateway surface
and as base64 / hex strings (per ts-rs binding) on the client surface;
the `Uint8Array` field annotation in the TS bindings is normative.

### `data` is adjacent-tagged

`data` is a serde adjacent-tagged enum. The outer key picks the
**surface**; the inner data is itself an adjacent-tagged enum picking
the **variant** within that surface:

```jsonc
{
  "id": <16 bytes>,
  "meta": { "kind": "command" },
  "data": {
    "type": "player",            // surface
    "data": {
      "event": "play",           // variant within surface
      "data": { "uri": "spotify:track:abc" }
    }
  }
}
```

The surfaces a gateway sees are catalogued under
`crates/lib/src/gateway/{from,to}/`; the surfaces a client sees are
under `crates/lib/src/client/{from,to}/`. The exact `type` strings and
`event` strings are camelCase renderings of the Rust variant names.

### `meta` semantics

```rust
enum MsgMeta {
  Command,
  Event,
  Request,
  Response { requestId: Uuid },
}
```

Four kinds, two roles per kind:

| `meta.kind` | sender intent                                                  | receiver contract                                                                |
| ----------- | -------------------------------------------------------------- | -------------------------------------------------------------------------------- |
| `command`   | "do this thing"                                                | act on it; no typed reply is part of the contract                                |
| `event`     | "this happened"                                                | observe; no reply                                                                |
| `request`   | "answer this; here's an `id` you'll echo back"                 | dispatch and produce **exactly one** matching `response` whose `requestId == id` |
| `response`  | "this is the answer to your earlier `request` whose id was..." | matched by the original requester's pending future                               |

The variant inside `data` constrains which `kind` is legal: a variant
that's a typed request (`#[bridge_request]` in the Rust source) is
sent with `kind = request` and answered on a sibling response variant
(`#[bridge_response]`). Codegen enforces the wiring at compile time on
the Rust side; the SDK helpers wrap the boilerplate on the TS / Swift
/ Kotlin side.

## requests, responses, and correlation

A typed request goes out as:

```jsonc
{ "id": <uuid A>, "meta": { "kind": "request" }, "data": { "type": "library", "data": { "event": "browse", "data": { "nodeId": null, "limit": 14, "offset": 0 } } } }
```

The responder replies with a fresh `id`, and the original request's id
appears under `meta.data.requestId`:

```jsonc
{ "id": <uuid B>, "meta": { "kind": "response", "data": { "requestId": <uuid A> } }, "data": { "type": "library", "data": { "event": "browseReply", "data": { "result": { "entries": [...] } } } } }
```

The requester correlates by `requestId` only; the response's own `id`
is just a fresh value for any future correlation.

**Timeout**: the daemon's request layer cancels pending requests after
**10 seconds** (`crates/core/src/bluetooth/mod.rs::REQUEST_TIMEOUT`). A
gateway that fails to respond inside that window will see its eventual
response dropped on the floor. Do not silently drop unhandled requests:
the requester's pending future surfaces a timeout to the caller.

**Auto-nack on unknown variants**: if the typed decode fails (the
sender named a variant the receiver doesn't know, e.g. a future-only
verb), the receiver auto-emits a `WireError::Unsupported` response
keyed off the request's id, harvested from the failed-decode "envelope
probe". The sender's pending future resolves immediately with
`RequestError::Protocol(WireError::Unsupported)` instead of waiting for
the 10s timeout.

## error shapes

There are two layers of error a request can come back as.

**`WireError`** is the protocol-level catalog, attached as the `Error`
variant on every `*MsgData` enum:

```rust
enum WireError {
  Unsupported,                      // variant not in receiver's vocabulary
  Unimplemented,                    // variant known but backend not wired
  Malformed { reason: String },     // payload couldn't be validated
  HandlerFailed { reason: String }, // unexpected internal error
}
```

`Unsupported` vs `Unimplemented` is the SDK's tell-apart between
"you're talking to a daemon that doesn't know this verb" (probably
old) and "this surface isn't wired yet" (probably under construction).

**Per-request domain errors** live inside the request's response
variant, not in `WireError`. For example `LibraryBrowseRequest` has a
`LibraryErrorReply` arm with its own enum (`NotFound`,
`PermissionDenied`, etc.) the gateway sends when a browse fails for a
predictable, op-specific reason. Domain errors model failures the
caller may want to recover from; `WireError` models protocol
violations.

The Rust SDK's `RequestError<E>` aggregates these into three cases:

```rust
enum RequestError<E> {
  Domain(E),              // the request's per-op error type
  Protocol(WireError),    // the universal catalog
  ResponseMismatch,       // wire shape didn't match what the request declared
}
```

Same model is exposed in the TS SDK.

## transports

### RFCOMM (production, on-device)

The canonical companion-to-daemon transport. Bluetooth Classic, msgpack
frames over a streaming RFCOMM socket, no encryption beyond what BlueZ
gives you.

| field          | value                                  |
| -------------- | -------------------------------------- |
| service UUID   | `dead0000-854d-408e-81f0-fb6147f918fd` |
| RFCOMM channel | 1                                      |
| device class   | `0x7c0000`                             |

Streaming framed by `BridgeEndec`. One BlueZ-paired peer = one
connection.

### iAP2 EA channel (production, iPhone)

Spotify-style "External Accessory" channel inside an iAP2 session,
running the same `BridgeEndec` frames. iOS gateways do not see
RFCOMM; iAP2 reconnect requires accessory-initiated dial (the daemon
opens the EA channel after iAP2 link-up).

### NetworkGateway WS (development / network-tethered)

Same wire frames over a WebSocket binary connection. Intended for dev
hosts that want to drive a Car Thing without Bluetooth pairing, and
for network-tethered companions on the same LAN.

| field         | value                                                                                 |
| ------------- | ------------------------------------------------------------------------------------- |
| port          | 8892                                                                                  |
| transport     | WebSocket binary frames, each carrying one or more `BridgeEndec` frames               |
| ws frame cap  | 1 MiB per WS message (covers single-frame assets + msgpack overhead)                  |
| address shape | each connecting peer is assigned a synthetic 6-byte address `0xfe:0xfe:<u32 counter>` |

The synthetic address shape means a single bridgething daemon can
multiplex BlueZ-paired peers and network peers in one address space;
gateways see themselves under whichever address the daemon assigns and
do not need to know which transport carried them.

### local client WebSocket (on-device webapps)

Modern third-party webapps and the bridgething launcher all connect to
the daemon's local WebSocket on **port 8891**, send / receive JSON
envelopes one-per-WS-text-frame. The envelope shape is identical to
the gateway surface (`id` / `meta` / `data`), but `data` types come
from `crates/lib/src/client/` instead of `crates/lib/src/gateway/`.

No magic byte, no compression byte, no priority byte; WS framing
handles all of that. The same `MsgMeta` and `WireError` types apply.

### stock Spotify WebSocket (legacy, port 8890)

The unmodifiable stock Spotify webapp connects on port 8890 with its
own legacy "interapp" protocol (string method names, untyped JSON).
The daemon translates these to modern surfaces; gateway and modern
webapp developers never see these messages. See
`docs/stock-webapp-gateway-contract.md` for how to populate the stock
webapp from a gateway.

## size and timeout limits

| limit                               | value   | enforced where                                                            |
| ----------------------------------- | ------- | ------------------------------------------------------------------------- |
| **request timeout**                 | 10 s    | `crates/core/src/bluetooth/mod.rs::REQUEST_TIMEOUT`                       |
| **single-frame `Asset.Push`**       | 256 KiB | `crates/lib/src/gateway/from/asset.rs::ASSET_PUSH_SINGLE_FRAME_MAX_BYTES` |
| **WS message cap (NetworkGateway)** | 1 MiB   | `crates/core/src/bluetooth/network/mod.rs::WS_MAX_FRAME_BYTES`            |
| **chunked transfer chunk size**     | 16 KiB  | application-layer convention; see `crates/lib/src/shared/asset.rs`        |

Anything above the single-frame asset cap **must** ship via
`AssetPushBegin` + `AssetPushChunk` + `AssetPushCommit` (the chunked
surface). The same shape applies to OTA payloads (`OtaBegin` /
`OtaChunk` / `OtaCommit`). Chunked transfers stream chunk-at-a-time to
disk on the daemon side; do not try to build up a giant single-frame
push and rely on the WS cap to "just work" — the daemon will reject it.

## versioning

The version byte is currently **2**. There is **no capability
negotiation**: a peer that ships a different version byte is dropped
on the floor with a `UnsupportedVersion` framing error. Mixed-version
deployments are not supported.

Version 1 was the pre-priority, msgpack-only framing. Version 2
introduced the encoding byte (header offset 4) and the priority byte
(header offset 5). The reserved bytes at offsets 6-7 are zero today
and held for future framing extensions.

## related references

- `crates/lib/src/protocol/` — canonical codec implementation
  (`bridge.rs`, `gateway.rs`, `frame.rs`, `probe.rs`).
- `crates/lib/src/wire.rs` — `MsgMeta`, `WireError`, `RequestError`,
  marker traits.
- `crates/lib/src/{client,gateway}/mod.rs` — outer envelope structs
  (`BridgeToGatewayMsg`, `ClientToBridgeMsg`, etc.) and the
  `BridgeOuterEnum`-derived surface enums.
- `crates/lib/src/shared/priority.rs` — priority lane.
- `crates/lib/ts/bindings/` — generated TS surface (regenerate with
  `just codegen`).
- `crates/lib/swift/Sources/` and `crates/lib/kotlin/schema/` —
  generated Swift and Kotlin surfaces.
- `docs/stock-webapp-gateway-contract.md` — populating the stock
  Spotify webapp from a gateway.
