//! Dispatch helper generation for the per-language gateway SDKs.
//!
//! Walks `crates/lib/src/` to learn:
//! - The two top-level wire enums (`BridgeToGatewayMsgData` and
//!   `GatewayToBridgeMsgData`) and their variants.
//! - Inner enums referenced by those variants.
//! - Marker-trait impls: `BridgeEvent`, `GatewayEvent`, `BridgeCommand`,
//!   `GatewayCommand`, `BridgeUnicast`, `GatewayUnicast`.
//! - Typed-request macro invocations: `impl_gateway_request!` /
//!   `impl_bridge_request!`.
//!
//! Then emits per-language helper files exposing surface-namespaced
//! typed dispatch (callbacks for TS, Flow for Kotlin, AsyncStream for
//! Swift).

pub mod inventory;
pub mod kotlin;
pub mod plan;
pub mod swift;
pub mod typescript;

pub use inventory::inventory;
pub use kotlin::emit_kotlin;
pub use plan::{Plan, build_plan};
pub use swift::emit_swift;
pub use typescript::emit_typescript;
