//! Category marker traits for gateway protocol surfaces.
//!
//! Every wire variant on either side of the gateway protocol is exactly
//! one of: an event (fire-and-forget), a command (fire-and-forget but
//! carries semantic that the receiver acts on it), a request (one-shot
//! request with a typed response), or a response (the typed reply to a
//! request). The first three are encoded as marker traits; responses
//! aren't independent surfaces — they're declared by the corresponding
//! request type.
//!
//! Naming follows the existing direction convention: `Bridge*` is
//! emitted by the daemon and consumed by the companion (sub-set of
//! `BridgeToGatewayMsgData`); `Gateway*` is emitted by the companion
//! and consumed by the daemon (sub-set of `GatewayToBridgeMsgData`).
//!
//! Codegen walks these impls to learn the category of each variant
//! and generate per-language dispatch helpers. Hand-written
//! send-callsite ergonomics on the daemon use the trait bounds
//! (`gateway.broadcast(event)`, `gateway.send_command(addr, cmd)`,
//! `gateway.request(addr, req).await`) so the meta is type-checked
//! at the call site rather than encoded by hand.

use crate::{
  BridgeThingMeta, ForwardMessage, GatewayMeta, NowPlayingUpdate,
  gateway::{
    BridgeToGatewayAssetMsg, BridgeToGatewayMsgData, BridgeToGatewayTransportMsg, GatewayError,
    GatewayToBridgeAssetMsg, GatewayToBridgeAuthorityMsg, GatewayToBridgeChromeMsg, GatewayToBridgeMsgData,
  },
};

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

// ----- BridgeEvent impls (daemon broadcasts) -----

impl BridgeEvent for BridgeThingMeta {}
impl BridgeEvent for BridgeToGatewayAssetMsg {}
impl BridgeEvent for ForwardMessage {}
impl BridgeEvent for GatewayError {}

// ----- BridgeCommand impls (daemon commands companion) -----

impl BridgeCommand for BridgeToGatewayTransportMsg {}

// ----- GatewayEvent impls (companion broadcasts) -----

impl GatewayEvent for GatewayMeta {}
impl GatewayEvent for GatewayToBridgeAssetMsg {}
impl GatewayEvent for GatewayToBridgeAuthorityMsg {}
impl GatewayEvent for NowPlayingUpdate {}

// ----- GatewayCommand impls (companion commands daemon) -----

impl GatewayCommand for GatewayToBridgeChromeMsg {}

/// Returns true when `data` is the response/error form of an
/// `impl_bridge_request!`-declared request. Used by the daemon's gateway
/// handler to drop response-shape arrivals that bypassed the pending
/// request match (timed-out, late reply, non-SDK companion sending the
/// response form as a fire-and-forget event). Without this filter, a
/// stray would dispatch into the per-surface handler and be hand-warned
/// per-arm — fine for asset today, but would have to grow with every
/// new `impl_bridge_request!`.
///
/// Add a `<surface>` arm here when a new bridge request lands whose
/// response variants share an inner enum with fire-and-forget variants.
/// Surfaces whose inner enum is response-only (today: none on this
/// direction, but webapp on the bridge → gateway direction is the
/// archetype) don't need an arm — their inner enum has no event marker
/// so codegen never emits a per-surface dispatcher to confuse.
pub fn is_response_variant(data: &GatewayToBridgeMsgData) -> bool {
  use crate::gateway::AssetRequest;
  match data {
    GatewayToBridgeMsgData::Asset(inner) => AssetRequest::is_response_variant(inner),
    _ => false,
  }
}
