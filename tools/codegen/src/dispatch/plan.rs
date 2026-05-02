//! Aggregates the inventory into per-surface buckets that the
//! per-language emitters consume directly. A "surface" maps to one
//! top-level wire variant (e.g. `Asset`) and bundles every method that
//! belongs in the `gateway.<surface>` namespace: inbound listeners,
//! outbound sends, typed queries, and typed inbound request handles.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Result, anyhow};

use super::inventory::{
  BRIDGE_TO_GATEWAY, EnumDef, GATEWAY_TO_BRIDGE, Inventory, MarkerKind, PayloadType, TypedRequest, WireVariant,
};

#[derive(Debug, Clone)]
pub struct DispatchEntry {
  /// Wire `data.type` discriminator (e.g. `"asset"`, `"transport"`).
  pub outer_disc: String,
  /// Outer variant name in PascalCase (e.g. `"Asset"`).
  pub outer_variant: String,
  /// Outer payload type — `None` for unit variants.
  pub outer_payload: Option<PayloadType>,
  /// Inner enum variants (when `outer_payload` is a `Named` type that
  /// resolves to an adjacent-tagged enum). Empty otherwise.
  pub inner_variants: Vec<InnerVariantPlan>,
  /// Inner enum's adjacent-tagged discriminator field name. `event` for
  /// most surfaces, `encoding` for `ForwardMessage`, `type` for
  /// `GatewayError`. `None` when there are no inner variants.
  pub inner_tag_field: Option<String>,
  /// Direction of the wire (which side receives this).
  pub direction: Direction,
  /// Event-vs-Command tag. Determines wire `meta` for outbound emit.
  pub category: EntryCategory,
  /// True when the variant is marked `BridgeUnicast` / `GatewayUnicast`.
  /// Unicast variants force deviceId at gateway-level send sites; the
  /// per-device proxy hides deviceId either way.
  pub unicast: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
  /// Daemon emits, companion receives.
  BridgeToGateway,
  /// Companion emits, daemon receives.
  GatewayToBridge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryCategory {
  Event,
  Command,
  Skip,
}

impl EntryCategory {
  pub fn meta_kind(self) -> &'static str {
    match self {
      Self::Event => "event",
      Self::Command => "command",
      Self::Skip => "event",
    }
  }
}

#[derive(Debug, Clone)]
pub struct InnerVariantPlan {
  pub disc: String,
  pub variant: String,
  pub payload: Option<PayloadType>,
  /// Outbound codegen skips struct variants — payload would need full
  /// per-field args.
  pub is_struct: bool,
}

pub struct Plan {
  pub entries: Vec<DispatchEntry>,
  /// Companion sends, daemon responds. Codegen emits typed query
  /// methods on the companion SDK.
  pub gateway_requests: Vec<TypedRequestEntry>,
  /// Daemon sends, companion responds. Codegen emits typed inbound
  /// handle pattern.
  pub bridge_requests: Vec<TypedRequestEntry>,
}

/// Per-language emitters consume `Surface`s — aggregations of
/// inbound + outbound entries for a single top-level wire variant
/// (e.g. all `Asset`-related methods for the `gateway.asset` namespace).
#[derive(Debug, Clone)]
pub struct Surface {
  /// PascalCase outer variant (e.g. `"Asset"`).
  pub name: String,
  /// camelCase property name on the gateway (e.g. `"asset"`).
  pub prop: String,
  /// Inbound dispatch entry — daemon → companion direction.
  pub inbound: Option<DispatchEntry>,
  /// Outbound dispatch entry — companion → daemon direction.
  pub outbound: Option<DispatchEntry>,
  /// Companion → daemon typed requests scoped to this surface.
  pub outbound_queries: Vec<TypedRequestEntry>,
  /// Daemon → companion typed requests scoped to this surface.
  pub inbound_requests: Vec<TypedRequestEntry>,
}

impl Surface {
  /// Inner variants of the inbound-side payload that should be exposed
  /// as event-shape callbacks. Excludes inner variants whose payload is
  /// a typed inbound request — those are handled via the handle pattern.
  pub fn inbound_event_variants(&self) -> Vec<&InnerVariantPlan> {
    let bridge_request_payloads: BTreeSet<&str> = self.inbound_requests.iter().map(|r| r.request.as_str()).collect();
    self
      .inbound
      .as_ref()
      .map(|e| {
        e.inner_variants
          .iter()
          .filter(|iv| match &iv.payload {
            Some(PayloadType::Named(n)) => !bridge_request_payloads.contains(n.as_str()),
            _ => true,
          })
          .collect()
      })
      .unwrap_or_default()
  }

  /// Inner variants of the outbound-side payload that should be
  /// exposed as outbound methods. Skips struct-shaped variants and
  /// any variant whose name matches a typed-request response/error
  /// (those flow through `handle.respond`).
  pub fn outbound_send_variants(&self) -> Vec<&InnerVariantPlan> {
    let mut response_variants: BTreeSet<String> = BTreeSet::new();
    for r in &self.inbound_requests {
      response_variants.insert(r.response_variant_pascal());
      if let Some(e) = r.error_variant_pascal() {
        response_variants.insert(e);
      }
    }
    self
      .outbound
      .as_ref()
      .map(|e| {
        e.inner_variants
          .iter()
          .filter(|iv| !iv.is_struct && !response_variants.contains(iv.variant.as_str()))
          .collect()
      })
      .unwrap_or_default()
  }
}

#[derive(Debug, Clone)]
pub struct TypedRequestEntry {
  pub request: String,
  pub request_takes_payload: bool,
  pub response: String,
  pub error: Option<String>,
  /// Outer variant in `<Direction>MsgData`, in PascalCase (e.g. `"Webapp"`).
  pub surface: String,
  /// Camel-case wire discriminator for the outer (e.g. `"webapp"`).
  pub surface_disc: String,
  /// Inner-enum adjacent-tagged tag field (e.g. `"event"`).
  pub inner_tag: String,
  /// Camel-case wire discriminator for the request inner variant.
  pub request_disc: String,
  pub response_disc: String,
  pub error_disc: Option<String>,
}

impl TypedRequestEntry {
  pub fn response_variant_pascal(&self) -> String {
    upper_first(&self.response_disc)
  }
  pub fn error_variant_pascal(&self) -> Option<String> {
    self.error_disc.as_deref().map(upper_first)
  }
  pub fn request_variant_pascal(&self) -> String {
    upper_first(&self.request_disc)
  }
}

pub fn upper_first(s: &str) -> String {
  let mut chars = s.chars();
  let mut out = String::new();
  if let Some(c) = chars.next() {
    out.extend(c.to_uppercase());
  }
  out.extend(chars);
  out
}

/// PascalCase -> camelCase, lower-casing the leading single capital.
/// `"Asset"` -> `"asset"`, `"NowPlayingUpdate"` -> `"nowPlayingUpdate"`.
pub fn rename_camel(s: &str) -> String {
  let mut out = String::with_capacity(s.len());
  let mut chars = s.chars();
  if let Some(first) = chars.next() {
    out.extend(first.to_lowercase());
  }
  out.extend(chars);
  out
}

/// Aggregate plan entries into per-surface buckets. Stable ordering:
/// top-level wire-variant order in `BridgeToGatewayMsgData` first,
/// then any outbound-only surfaces, then any surfaces that appear
/// only via typed requests.
pub fn surfaces(plan: &Plan) -> Vec<Surface> {
  let mut by_name: BTreeMap<String, Surface> = BTreeMap::new();
  let mut order: Vec<String> = Vec::new();

  let touch = |name: &str, by_name: &mut BTreeMap<String, Surface>, order: &mut Vec<String>| {
    if !by_name.contains_key(name) {
      by_name.insert(
        name.to_string(),
        Surface {
          name: name.to_string(),
          prop: rename_camel(name),
          inbound: None,
          outbound: None,
          outbound_queries: Vec::new(),
          inbound_requests: Vec::new(),
        },
      );
      order.push(name.to_string());
    }
  };

  for e in plan
    .entries
    .iter()
    .filter(|e| e.direction == Direction::BridgeToGateway)
  {
    touch(&e.outer_variant, &mut by_name, &mut order);
    by_name.get_mut(&e.outer_variant).unwrap().inbound = Some(e.clone());
  }
  for e in plan
    .entries
    .iter()
    .filter(|e| e.direction == Direction::GatewayToBridge)
  {
    touch(&e.outer_variant, &mut by_name, &mut order);
    by_name.get_mut(&e.outer_variant).unwrap().outbound = Some(e.clone());
  }
  for r in &plan.gateway_requests {
    touch(&r.surface, &mut by_name, &mut order);
    by_name.get_mut(&r.surface).unwrap().outbound_queries.push(r.clone());
  }
  for r in &plan.bridge_requests {
    touch(&r.surface, &mut by_name, &mut order);
    by_name.get_mut(&r.surface).unwrap().inbound_requests.push(r.clone());
  }

  order.into_iter().map(|n| by_name.remove(&n).unwrap()).collect()
}

pub fn build_plan(inv: &Inventory) -> Result<Plan> {
  let mut entries = Vec::new();
  for (wire_name, dir) in [
    (BRIDGE_TO_GATEWAY, Direction::BridgeToGateway),
    (GATEWAY_TO_BRIDGE, Direction::GatewayToBridge),
  ] {
    let wire = inv
      .wire_enums
      .get(wire_name)
      .ok_or_else(|| anyhow!("dispatch: missing wire enum {wire_name}"))?;

    for variant in &wire.variants {
      let category = classify_outer_variant(variant, dir, inv);
      if matches!(category, EntryCategory::Skip) {
        continue;
      }
      let unicast = is_unicast(variant, dir, inv);
      let inner_enum = variant.payload.as_ref().and_then(|p| match p {
        PayloadType::Named(n) => inv.enums.get(n),
        _ => None,
      });
      let inner_variants = inner_enum.map(inner_variant_plans).unwrap_or_default();
      let inner_tag_field = inner_enum.map(|en| en.tag_field.clone());
      entries.push(DispatchEntry {
        outer_disc: rename_camel(&variant.name),
        outer_variant: variant.name.clone(),
        outer_payload: variant.payload.clone(),
        inner_variants,
        inner_tag_field,
        direction: dir,
        category,
        unicast,
      });
    }
  }

  let gateway_requests = inv
    .gateway_requests
    .iter()
    .filter_map(|r| build_typed_request(r, inv, RequestDirection::GatewayToBridge))
    .collect();
  let bridge_requests = inv
    .bridge_requests
    .iter()
    .filter_map(|r| build_typed_request(r, inv, RequestDirection::BridgeToGateway))
    .collect();

  Ok(Plan {
    entries,
    gateway_requests,
    bridge_requests,
  })
}

fn inner_variant_plans(en: &EnumDef) -> Vec<InnerVariantPlan> {
  en.variants
    .iter()
    .map(|v| InnerVariantPlan {
      disc: rename_camel(&v.name),
      variant: v.name.clone(),
      payload: v.payload.clone(),
      is_struct: v.is_struct,
    })
    .collect()
}

#[derive(Clone, Copy)]
enum RequestDirection {
  GatewayToBridge,
  BridgeToGateway,
}

fn build_typed_request(r: &TypedRequest, inv: &Inventory, dir: RequestDirection) -> Option<TypedRequestEntry> {
  let inner_enum_name = match dir {
    RequestDirection::GatewayToBridge => format!("GatewayToBridge{}Msg", r.surface),
    RequestDirection::BridgeToGateway => format!("BridgeToGateway{}Msg", r.surface),
  };
  let response_inner_enum_name = match dir {
    RequestDirection::GatewayToBridge => format!("BridgeToGateway{}Msg", r.surface),
    RequestDirection::BridgeToGateway => format!("GatewayToBridge{}Msg", r.surface),
  };
  let inner_tag = inv
    .enums
    .get(&inner_enum_name)
    .or_else(|| inv.enums.get(&response_inner_enum_name))
    .map(|e| e.tag_field.clone())
    .unwrap_or_else(|| "event".to_string());
  Some(TypedRequestEntry {
    request: r.request.clone(),
    request_takes_payload: r.request_takes_payload,
    response: r.response.clone(),
    error: r.error.clone(),
    surface: r.surface.clone(),
    surface_disc: rename_camel(&r.surface),
    inner_tag,
    request_disc: rename_camel(&r.request_variant),
    response_disc: rename_camel(&r.response_variant),
    error_disc: r.error_variant.as_deref().map(rename_camel),
  })
}

fn classify_outer_variant(variant: &WireVariant, dir: Direction, inv: &Inventory) -> EntryCategory {
  let Some(PayloadType::Named(payload)) = variant.payload.as_ref() else {
    return EntryCategory::Skip;
  };
  let kinds = inv.markers.get(payload).cloned().unwrap_or_default();
  let event_kind = match dir {
    Direction::BridgeToGateway => MarkerKind::BridgeEvent,
    Direction::GatewayToBridge => MarkerKind::GatewayEvent,
  };
  let cmd_kind = match dir {
    Direction::BridgeToGateway => MarkerKind::BridgeCommand,
    Direction::GatewayToBridge => MarkerKind::GatewayCommand,
  };
  if kinds.contains(&cmd_kind) {
    EntryCategory::Command
  } else if kinds.contains(&event_kind) {
    EntryCategory::Event
  } else {
    EntryCategory::Skip
  }
}

fn is_unicast(variant: &WireVariant, dir: Direction, inv: &Inventory) -> bool {
  let Some(PayloadType::Named(payload)) = variant.payload.as_ref() else {
    return false;
  };
  let kinds = inv.markers.get(payload).cloned().unwrap_or_default();
  let unicast_kind = match dir {
    Direction::BridgeToGateway => MarkerKind::BridgeUnicast,
    Direction::GatewayToBridge => MarkerKind::GatewayUnicast,
  };
  kinds.contains(&unicast_kind)
}
