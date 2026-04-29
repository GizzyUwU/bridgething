# bridgething agent guide

Rules an agent has to follow when editing this repo. The codebase has a
clean lib/core split that is easy to silently break — these rules exist
to keep that split intact.

## Crate layout and what goes where

This is a cargo workspace: `lib/`, `core/`, `client/rust/`, `adapters/node/`.

### lib/ (`libbridgething`) — wire surface only

Anything that crosses a websocket or bluetooth boundary lives here, plus
the codec/framing for those boundaries. That's it.

Allowed in lib:
- Wire DTOs (every type that gets serialized to msgpack on the BT link
  or to JSON on the local websocket).
- The codec / framing in `lib/src/protocol/`.
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

### core/ (`bridgething`) — the daemon

Everything else. Handlers, managers, axum server, BlueR plumbing,
chromium CDP driver, persistent state, hardware drivers (ALS, mic),
systemd integration. The binary lives here.

Core depends on lib for wire types and re-exports nothing.

### client/rust/, adapters/node/

Both consume `libbridgething`. They MUST NOT redefine wire types — they
re-export from lib or build on top of lib's types. If a wire type needs
a field added, the field goes in lib and propagates outward, never the
other way.

## Wrap, don't duplicate

If a runtime variant of an enum doesn't fit a wire enum, do not copy
the wire enum into core and add the variant. Wrap it.

Example of the right pattern (already in `core/src/handler/client/msg.rs`):

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
in core — they reuse the lib type. Only the *enum shell* is core-side
because it has runtime-only variants.

The cautionary tale: `WebappInfo` was at one point copy-pasted
identically between lib and core. There is exactly one canonical home
for any wire type: `lib/`. Core imports it.

If you find yourself writing a `struct` or `enum` in core that has the
same fields (or near-identical fields) as one in lib, stop. The lib
type either:
- already does what you need — import it, or
- needs a runtime extension — write a wrapper that holds it.

## Stock translation lives in core, deliberately

The stock Spotify webapp's wire protocol (raw JSON the unmodifiable
stock webapp emits and consumes) lives in `core/src/stock/` and the
dispatcher that translates it lives in `core/src/handler/client/stock.rs`.
This is the one apparent exception to "wire types live in lib" and it
is intentional.

Why: the SDKs that consume `libbridgething` (gateway, mobile apps,
on-device clients) don't speak stock. Stock is a translation layer at
the daemon edge — modern shapes go over BT, stock JSON only ever lands
on a local websocket from the stock webapp. Putting stock in lib would
pollute every generated TS / Swift / Kotlin binding with types those
consumers will never use.

There IS a `lib/src/stock/` module with a small handful of types
(`StockSetPreset`, `StockPreset`). Those are not the same thing — they
are SDK-facing types that a *modern* webapp uses to invoke legacy
operations through `ClientCommandType::LegacyStock`. The rule:

- `lib/src/stock/` = SDK-facing types for legacy operations a modern
  webapp may want to invoke. Generated TS / Swift / Kotlin gets these.
- `core/src/stock/` = wire shapes for the stock Spotify webapp. Never
  leaves the daemon.

If you're adding a type because the stock webapp sends or receives it,
that goes in `core/src/stock/`. If you're adding a type that a modern
webapp wants to access from its TypeScript code, that goes in
`lib/src/stock/`.

## shared/ in lib

`lib/src/shared/` is for types used by both directions of a wire
protocol (gateway↔bridge, client↔server) AND used by more than one
protocol or surface. Examples that belong: `Track`, `Album`, `Device`,
`ForwardMessage`, `WebappInfo`.

If a type is only used in one direction or only by one protocol, it
goes in that direction's module. Don't promote to `shared/` for tidiness.

## Codegen — run `just codegen`, never hand-edit generated files

`lib/ts/bindings/`, `lib/swift/Sources/BridgethingSchema/Generated.swift`,
and `lib/kotlin/.../Generated.kt` are emitted by `just codegen`, which
runs the `bridgething-codegen` tool and then prettier. Generated files
have no human edits. If a generated file is wrong, the fix goes in:

1. The Rust source in `lib/src/` (annotations, types).
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
`lib/kotlin/schema/src/main/kotlin/dev/bridgething/schema/Serializers.kt`.
The build will tell you what's missing.

## Naming gotchas

- "client" is overloaded. In bridgething's wire protocol it means *the
  on-device webapp talking to the daemon over local websocket*. In any
  other context "client" is ambiguous. When writing comments or docs,
  prefer "webapp" or "on-device client" if there's any chance of
  confusion.
- `lib::server::ServerEvent` is a wire type (bridge → webapp event).
  `core::http::Server` is the actual axum HTTP+WS server (`core/src/http/`,
  ports 8890/8891). Comments and docs should not say "the server" without
  qualifying which one.
- `lib::gateway` ↔ `core::handler::gateway` is a 1:1 mapping: every
  bridge↔gateway wire variant in lib has a handler in core. When adding
  a wire variant, add the handler in the same change.
- `lib::client` ↔ `core::handler::client` is the same 1:1 mapping for
  the local websocket protocol. Runtime types that wrap lib's wire enum
  (`RecvMsgData`, `PossibleRecvMsg`, `ClientMode`, etc.) live in
  `core/src/handler/client/msg.rs` and are re-exported from
  `core::handler::client`.

## Workspace ergonomics

- `cargo run -p bridgething` runs the daemon against dev paths under
  `~/.local/share/bridgething/` and `~/.config/bridgething/`. See
  `core/src/paths.rs` for env var overrides.
- `cargo build -p bridgething --features superbird --no-default-features`
  is the on-device build (drops dev-host features). Cross builds use
  `cross` with the same flags.
- `cargo test -p libbridgething` runs unit + golden tests. The golden
  fixtures live in `lib/tests/`; regenerate with
  `UPDATE_GOLDEN=1 cargo test -p libbridgething --test golden`.
- `just codegen` after any change to a lib type that crosses to TS /
  Swift / Kotlin.
