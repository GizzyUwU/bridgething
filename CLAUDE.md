# bridgething agent guide

Rules an agent has to follow when editing this repo. The codebase has a
clean lib/core split that is easy to silently break - these rules exist
to keep that split intact.

## Crate layout and what goes where

The repo splits into two top-level workspace roots:

- `crates/` — Rust workspace members (`crates/lib`, `crates/core`, `crates/client-rs`, `crates/mfi`, `crates/mfi-proxy`, ...). The cargo workspace also pulls in `packages/adapter-node/` (mixed Rust+TS via NAPI) and `tools/codegen/`.
- `packages/` — Bun/turbo workspace members (`packages/gateway/typescript`, `packages/client-ts`, `packages/adapter-node`, `packages/adapter-rn`, `packages/examples/*`).
- `mobile/` — RN app, consumer of the packages.

The lib/core split below is the load-bearing one. Naming convention: Rust crates use kebab-case package names (`bridgething-mfi`); TS packages use scoped names (`@bridgething/lib`).

### crates/lib/ (`libbridgething`) — wire surface only

Anything that crosses a websocket or bluetooth boundary lives here, plus
the codec/framing for those boundaries. That's it.

Allowed in lib:

- Wire DTOs (every type that gets serialized to msgpack on the BT link
  or to JSON on the local websocket).
- The codec / framing in `crates/lib/src/protocol/`.
- Compile-time constants used by the protocol (UUIDs, ports, class IDs).
- `serde`, `ts-rs`, `typeshare`, `uuid`, `serde_with`, `derive_more`,
  and the `protocol` feature deps (tokio-util, flate2, rmp-serde).

Forbidden in lib:

- Tokio runtime types (`tokio::sync::mpsc`, `tokio::task`, etc).
- Handlers, managers, daemon state, hardware drivers.
- Errors that aren't pure protocol errors. `EndecError` is fine.
  `BluetoothError` is not.
- Anything that would not be useful to a third party importing
  `libbridgething` purely to speak the wire protocol.

If you reach for `tokio::sync::mpsc::Sender<Foo>` in lib, stop — that
type belongs in core.

### crates/core/ (`bridgething`) — the daemon

Everything else. Handlers, managers, axum server, BlueR plumbing,
chromium CDP driver, persistent state, hardware drivers (ALS, mic),
systemd integration. The binary lives here.

Core depends on lib for wire types and re-exports nothing.

### crates/client-rs/, packages/adapter-node/

Both consume `libbridgething`. They MUST NOT redefine wire types — they
re-export from lib or build on top of lib's types. If a wire type needs
a field added, the field goes in lib and propagates outward, never the
other way.

## Wrap, don't duplicate

If a runtime variant of an enum doesn't fit a wire enum, do not copy
the wire enum into core and add the variant. Wrap it.

Example of the right pattern (already in `crates/core/src/handler/client/msg.rs`):

```rust
// lib::ClientCommandType is the wire enum.
// core::RecvMsgData wraps it and adds runtime-only variants.
pub enum RecvMsgData {
  Bluetooth(ClientBluetoothCommand),    // re-projected from lib variant
  // ...
  Hole,                                 // runtime only
  Unsupported(PossibleRecvMsg),         // runtime only
  ChangeMode(ClientMode),               // runtime only
  ConnectionClosed(u16, String),        // runtime only
}
```

The fields inside `Bluetooth(ClientBluetoothCommand)` are NOT redefined
in core — they reuse the lib type. Only the _enum shell_ is core-side
because it has runtime-only variants.

The cautionary tale: `WebappInfo` was at one point copy-pasted
identically between lib and core. There is exactly one canonical home
for any wire type: `crates/lib/`. Core imports it.

If you find yourself writing a `struct` or `enum` in core that has the
same fields (or near-identical fields) as one in lib, stop. The lib
type either:

- already does what you need — import it, or
- needs a runtime extension — write a wrapper that holds it.

## Stock translation lives in core, deliberately

The stock Spotify webapp's wire protocol (raw JSON the unmodifiable
stock webapp emits and consumes) lives in `crates/core/src/stock/` and the
dispatcher that translates it lives in `crates/core/src/handler/client/stock.rs`.
This is the one apparent exception to "wire types live in lib" and it
is intentional.

Why: the SDKs that consume `libbridgething` (gateway, mobile apps,
on-device clients) don't speak stock. Stock is a translation layer at
the daemon edge — modern shapes go over BT, stock JSON only ever lands
on a local websocket from the stock webapp. Putting stock in lib would
pollute every generated TS / Swift / Kotlin binding with types those
consumers will never use.

There IS a `crates/lib/src/stock/` module with a small handful of types
(`StockSetPreset`, `StockPreset`). Those are not the same thing — they
are SDK-facing types that a _modern_ webapp uses to invoke legacy
operations through `ClientCommandType::LegacyStock`. The rule:

- `crates/lib/src/stock/` = SDK-facing types for legacy operations a modern
  webapp may want to invoke. Generated TS / Swift / Kotlin gets these.
- `crates/core/src/stock/` = wire shapes for the stock Spotify webapp. Never
  leaves the daemon.

If you're adding a type because the stock webapp sends or receives it,
that goes in `crates/core/src/stock/`. If you're adding a type that a
modern webapp wants to access from its TypeScript code, that goes in
`crates/lib/src/stock/`.

## shared/ in lib

`crates/lib/src/shared/` is for types used by both directions of a wire
protocol (gateway↔bridge, client↔server) AND used by more than one
protocol or surface. Examples that belong: `Track`, `Album`, `Device`,
`ForwardMessage`, `WebappInfo`.

If a type is only used in one direction or only by one protocol, it
goes in that direction's module. Don't promote to `shared/` for tidiness.

## Codegen — run `just codegen`, never hand-edit generated files

`crates/lib/ts/bindings/`, `crates/lib/swift/Sources/BridgethingSchema/Generated.swift`,
and `crates/lib/kotlin/.../Generated.kt` are emitted by `just codegen`,
which runs the `bridgething-codegen` tool and then prettier. Generated
files have no human edits. If a generated file is wrong, the fix goes in:

1. The Rust source in `crates/lib/src/` (annotations, types).
2. The codegen tool in `tools/codegen/` (post-processing transforms).

Never add another perl/sed one-liner to the Justfile to patch generated
output. Every transform belongs in the codegen tool, where it is
discoverable, testable, and reviewed alongside the type that needs it.

The kotlin side has a recurring pattern: any adjacent-tagged enum
(`#[serde(tag = "type", content = "data")]`) emits as a sealed class
that needs an `AdjacentTaggedSerializer` proxy in `Serializers.kt`.
The codegen tool discovers these automatically by parsing lib sources;
if you add a new adjacent-tagged enum, you do not need to register it
anywhere — but you DO need to add the matching `XSerializer` object to
`crates/lib/kotlin/schema/src/main/kotlin/dev/bridgething/schema/Serializers.kt`.
The build will tell you what's missing.

## Concurrency + ownership

The daemon is actor-style: one tokio task owns its state, and every state
transition flows through messages, not locks. `RfcommGateway` in
`crates/core/src/bluetooth/rfcomm/mod.rs` is the canonical example to
copy when building a new subsystem.

Rules:

- **One task owns its data; other tasks reach it via mpsc.** In
  `RfcommGateway::recv`, the `connections: HashMap<Address, Connection>`
  is owned by the `recv()` task and only mutated inside its `select!`
  loop. Outside callers post messages through `GatewaySendTx`. Do not
  reach for `Arc<Mutex<Foo>>` to "share" Foo — either Foo's owner
  spawns a task and accepts commands by mpsc, or Foo is cheaply
  cloneable wire data.
- **`Arc<RwLock<...>>` exists in this codebase but is the exception.**
  `state::AppState` and `ProfileManager::profile_state` both wrap
  read-mostly snapshot state read by many surfaces (HTTP, BT, agent).
  New subsystems should reach for an mpsc before reaching for a lock;
  locks are tolerated when the data is read-mostly snapshot, not when
  it's the hot path of a protocol state machine.
- **`init() -> Self` and `spawn(self) -> JoinHandle<()>` are separate.**
  Construction takes ports, registers BlueZ profiles, opens sockets;
  `spawn` consumes the struct and returns a handle so the daemon can
  decide when loops start. See `RfcommGateway::init` /
  `RfcommGateway::spawn`. Background tasks return `JoinHandle<()>`
  stored in `_handle: JoinHandle<()>` fields — the underscore says
  "kept alive to drop on Drop, never `.await`ed".
- **Bounded mpsc, default capacity 16.** `tokio::sync::mpsc::channel(16)`
  appears in `BluetoothManager::init`, `RfcommGateway::init`,
  `GatewayCon::init`. The bound is the backpressure — propagate
  upstream rather than allocating an unbounded buffer.
- **Byte-stream protocols use `tokio_util::codec::Framed`.** The rfcomm
  gateway frames `BridgeEndec` over `bluer::rfcomm::Stream`; the iap2
  link layer frames `LinkCodec` over the same. Implement `Decoder` to
  return `Ok(None)` until enough bytes are buffered, and to advance one
  byte on resync paths. `Framed::split()` gives `Sink + Stream` halves
  you can move into separate tasks.
- **Heap discipline on protocol code.** Use `bytes::Bytes` for
  zero-copy shared-readonly buffers and `bytes::BytesMut` for
  build-phase. Avoid `Vec<u8>` clones for in-flight payloads:
  retransmit queues hold `Bytes` (refcount-only clone), CSM builders
  write into a single `BytesMut` then freeze.
- **Tracing density matches `RfcommGateway`'s.** Every state transition
  at `debug!`, every frame at `trace!`, every successful handshake /
  connection-up at `info!`, every error path at `warn!` or `error!`.
- **`thiserror` enums with `#[from]` for error conversion.** The shape
  of `BluetoothError` is the model: each subsystem gets its own enum,
  and the larger error types `#[from]` the smaller ones so `?` works
  through layers.

The Car Thing has 512 MB of RAM shared with chromium and the kiosk web
app. Heap clones, unbounded queues, and mutex contention all cost real
frames. The actor-with-bounded-mpsc shape is what keeps the hot paths
cheap.

## Naming gotchas

- "client" is overloaded. In bridgething's wire protocol it means _the
  on-device webapp talking to the daemon over local websocket_. In any
  other context "client" is ambiguous. When writing comments or docs,
  prefer "webapp" or "on-device client" if there's any chance of
  confusion.
- `lib::server::ServerEvent` is a wire type (bridge → webapp event).
  `core::http::Server` is the actual axum HTTP+WS server
  (`crates/core/src/http/`, ports 8890/8891). Comments and docs should not
  say "the server" without qualifying which one.
- `lib::gateway` ↔ `core::handler::gateway` is a 1:1 mapping: every
  bridge↔gateway wire variant in lib has a handler in core. When adding
  a wire variant, add the handler in the same change.
- `lib::client` ↔ `core::handler::client` is the same 1:1 mapping for
  the local websocket protocol. Runtime types that wrap lib's wire enum
  (`RecvMsgData`, `PossibleRecvMsg`, `ClientMode`, etc.) live in
  `crates/core/src/handler/client/msg.rs` and are re-exported from
  `core::handler::client`.

## Workspace ergonomics

- `cargo run -p bridgething` runs the daemon against dev paths under
  `~/.local/share/bridgething/` and `~/.config/bridgething/`. See
  `crates/core/src/paths.rs` for env var overrides.
- `cargo build -p bridgething --features superbird --no-default-features`
  is the on-device build (drops dev-host features). Cross builds use
  `cross` with the same flags.
- `cargo test -p libbridgething` runs unit + golden tests. The golden
  fixtures live in `crates/lib/tests/`; regenerate with
  `UPDATE_GOLDEN=1 cargo test -p libbridgething --test golden`.
- `just codegen` after any change to a lib type that crosses to TS /
  Swift / Kotlin.
- No emdashes or endashes.
