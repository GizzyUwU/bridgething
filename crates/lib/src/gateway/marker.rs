//! Category marker traits for gateway protocol surfaces.
//!
//! Every wire variant on either side of the gateway protocol is exactly
//! one of: an event (fire-and-forget), a command (fire-and-forget but
//! carries semantic that the receiver acts on it), a request (one-shot
//! request with a typed response), or a response (the typed reply to a
//! request). The first two are encoded as marker traits whose impls
//! land on per-bucket sibling enums emitted by `#[derive(BridgeEnum)]`
//! (or directly on top-level outer-wire variant payload types via the
//! standalone `#[derive(BridgeEvent)]` / `#[derive(GatewayEvent)]` /
//! `#[derive(BridgeCommand)]` / `#[derive(GatewayCommand)]` derives).
//! Requests and responses route through `BridgeRequest` /
//! `GatewayRequest` (see `request.rs`); responses don't get a marker.
//!
//! Naming follows the existing direction convention: `Bridge*` is
//! emitted by the daemon and consumed by the companion (sub-set of
//! `BridgeToGatewayMsgData`); `Gateway*` is emitted by the companion
//! and consumed by the daemon (sub-set of `GatewayToBridgeMsgData`).
//!
//! Hand-written send-callsite ergonomics on the daemon use the trait
//! bounds (`gateway.broadcast(event)`, `gateway.send_command(addr, cmd)`,
//! `gateway.request(addr, req).await`) so the meta is type-checked at
//! the call site rather than encoded by hand.

use crate::gateway::{BridgeToGatewayMsgData, GatewayToBridgeMsgData};

/// bridge → gateway: a fire-and-forget event broadcast by the daemon.
/// Receiver does whatever it wants with it; no reply expected.
pub trait BridgeEvent: Into<BridgeToGatewayMsgData> {}

/// gateway → bridge: a fire-and-forget event broadcast by the companion.
pub trait GatewayEvent: Into<GatewayToBridgeMsgData> {}

/// bridge → gateway: a command the companion is expected to action.
/// Wire `meta = command`; receiver may Ack to confirm receipt but no
/// typed response is part of the contract.
pub trait BridgeCommand: Into<BridgeToGatewayMsgData> {}

/// gateway → bridge: a command the daemon is expected to action.
pub trait GatewayCommand: Into<GatewayToBridgeMsgData> {}

/// bridge → gateway: a fire-and-forget event/command that must be
/// addressed to a specific peer. Pair with `BridgeEvent` or
/// `BridgeCommand`. Without this marker, codegen treats the variant as
/// broadcastable and omits the deviceId param at gateway level (the
/// per-device proxy always carries deviceId implicitly).
pub trait BridgeUnicast: Into<BridgeToGatewayMsgData> {}

/// gateway → bridge: companion-side analogue of `BridgeUnicast`.
pub trait GatewayUnicast: Into<GatewayToBridgeMsgData> {}
