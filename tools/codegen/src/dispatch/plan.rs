use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Result, anyhow};

use super::inventory::{
  Direction, EnumDef, Inventory, MarkerKind, PayloadType, Protocol, TypedRequest, VariantTag, WireVariant,
};

#[derive(Debug, Clone)]
pub struct DispatchEntry {
  pub outer_disc: String,
  pub outer_variant: String,
  pub outer_payload: Option<PayloadType>,
  pub inner_variants: Vec<InnerVariantPlan>,
  pub inner_tag_field: Option<String>,
  pub direction: Direction,
  pub category: EntryCategory,
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
  pub is_struct: bool,
  pub boxed: bool,
  pub category: Option<EntryCategory>,
}

pub struct Plan {
  pub protocol: Protocol,
  pub entries: Vec<DispatchEntry>,
  pub outbound_requests: Vec<TypedRequestEntry>,
  pub inbound_requests: Vec<TypedRequestEntry>,
}

#[derive(Debug, Clone)]
pub struct Surface {
  pub name: String,
  pub prop: String,
  pub inbound: Option<DispatchEntry>,
  pub outbound: Option<DispatchEntry>,
  pub outbound_queries: Vec<TypedRequestEntry>,
  pub inbound_requests: Vec<TypedRequestEntry>,
}

impl Surface {
  pub fn inbound_event_variants(&self) -> Vec<&InnerVariantPlan> {
    self
      .inbound
      .as_ref()
      .map(|e| {
        e.inner_variants
          .iter()
          .filter(|iv| !self.is_inbound_request_variant(&iv.variant, iv.payload.as_ref()))
          .collect()
      })
      .unwrap_or_default()
  }

  pub fn is_inbound_request_variant(&self, variant: &str, payload: Option<&PayloadType>) -> bool {
    let by_variant = self
      .inbound_requests
      .iter()
      .any(|r| r.request_variant_pascal() == variant);
    let by_payload = matches!(payload, Some(PayloadType::Named(n))
      if self.inbound_requests.iter().any(|r| r.request == *n));
    by_variant || by_payload
  }

  pub fn outbound_send_variants(&self) -> Vec<&InnerVariantPlan> {
    let mut response_variants: BTreeSet<String> = BTreeSet::new();
    for r in &self.inbound_requests {
      response_variants.insert(r.response_variant_pascal());
      if let Some(e) = r.error_variant_pascal() {
        response_variants.insert(e);
      }
    }
    let mut request_variants: BTreeSet<String> = BTreeSet::new();
    for r in &self.outbound_queries {
      request_variants.insert(r.request_variant_pascal());
    }
    self
      .outbound
      .as_ref()
      .map(|e| {
        e.inner_variants
          .iter()
          .filter(|iv| {
            !iv.is_struct
              && !response_variants.contains(iv.variant.as_str())
              && !request_variants.contains(iv.variant.as_str())
          })
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
  pub surface: String,
  pub surface_disc: String,
  pub inner_tag: String,
  pub request_disc: String,
  pub response_disc: String,
  pub error_disc: Option<String>,
}

impl TypedRequestEntry {
  pub fn is_bulk_byte_stream(&self) -> bool {
    self.surface_disc == "asset"
  }

  pub fn is_concurrent(&self) -> bool {
    matches!(
      (self.surface_disc.as_str(), self.request_disc.as_str()),
      ("asset", "request") | ("system", "otaAssetRange")
    )
  }

  pub fn carries_request_id(&self) -> bool {
    matches!(
      (self.surface_disc.as_str(), self.request_disc.as_str()),
      ("system", "otaAssetRange")
    )
  }

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

pub fn rename_camel(s: &str) -> String {
  let mut out = String::with_capacity(s.len());
  let mut chars = s.chars();
  if let Some(first) = chars.next() {
    out.extend(first.to_lowercase());
  }
  out.extend(chars);
  out
}

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

  let inbound_dir = plan.protocol.inbound_direction();
  let outbound_dir = plan.protocol.outbound_direction();

  for e in plan.entries.iter().filter(|e| e.direction == inbound_dir) {
    touch(&e.outer_variant, &mut by_name, &mut order);
    by_name.get_mut(&e.outer_variant).unwrap().inbound = Some(e.clone());
  }
  for e in plan.entries.iter().filter(|e| e.direction == outbound_dir) {
    touch(&e.outer_variant, &mut by_name, &mut order);
    by_name.get_mut(&e.outer_variant).unwrap().outbound = Some(e.clone());
  }
  for r in &plan.outbound_requests {
    touch(&r.surface, &mut by_name, &mut order);
    by_name.get_mut(&r.surface).unwrap().outbound_queries.push(r.clone());
  }
  for r in &plan.inbound_requests {
    touch(&r.surface, &mut by_name, &mut order);
    by_name.get_mut(&r.surface).unwrap().inbound_requests.push(r.clone());
  }

  order.into_iter().map(|n| by_name.remove(&n).unwrap()).collect()
}

impl Protocol {
  pub fn inbound_direction(self) -> Direction {
    match self {
      Self::Gateway => Direction::BridgeToGateway,
      Self::Client => Direction::BridgeToClient,
    }
  }

  pub fn outbound_direction(self) -> Direction {
    match self {
      Self::Gateway => Direction::GatewayToBridge,
      Self::Client => Direction::ClientToBridge,
    }
  }
}

pub fn build_plans(inv: &Inventory) -> Result<Vec<Plan>> {
  let protocols = [Protocol::Gateway, Protocol::Client];
  let mut plans = Vec::new();
  for protocol in protocols {
    plans.push(build_plan_for(inv, protocol)?);
  }
  Ok(plans)
}

pub fn build_plan_for(inv: &Inventory, protocol: Protocol) -> Result<Plan> {
  let inbound_dir = protocol.inbound_direction();
  let outbound_dir = protocol.outbound_direction();

  let mut entries = Vec::new();
  for direction in [inbound_dir, outbound_dir] {
    let wire_name = direction.wire_data_name();
    let wire = inv
      .wire_enums
      .get(wire_name)
      .ok_or_else(|| anyhow!("dispatch: missing wire enum {wire_name}"))?;

    for variant in &wire.variants {
      let category = classify_outer_variant(variant, direction, inv);
      if matches!(category, EntryCategory::Skip) {
        continue;
      }
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
        direction,
        category,
      });
    }
  }

  let outbound_requests = inv
    .typed_requests
    .iter()
    .filter(|r| r.direction == outbound_dir)
    .filter_map(|r| build_typed_request(r, inv))
    .collect();
  let inbound_requests = inv
    .typed_requests
    .iter()
    .filter(|r| r.direction == inbound_dir)
    .filter_map(|r| build_typed_request(r, inv))
    .collect();

  Ok(Plan {
    protocol,
    entries,
    outbound_requests,
    inbound_requests,
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
      boxed: v.boxed,
      category: match v.tag {
        Some(VariantTag::Event) => Some(EntryCategory::Event),
        Some(VariantTag::Command) => Some(EntryCategory::Command),
        Some(VariantTag::Request) | Some(VariantTag::Response) | None => None,
      },
    })
    .collect()
}

fn build_typed_request(r: &TypedRequest, inv: &Inventory) -> Option<TypedRequestEntry> {
  let request_inner_name = format!(
    "{}{}Msg",
    match r.direction {
      Direction::BridgeToGateway => "BridgeToGateway",
      Direction::GatewayToBridge => "GatewayToBridge",
      Direction::BridgeToClient => "BridgeToClient",
      Direction::ClientToBridge => "ClientToBridge",
    },
    r.surface
  );
  let response_inner_name = format!(
    "{}{}Msg",
    match r.direction.opposite() {
      Direction::BridgeToGateway => "BridgeToGateway",
      Direction::GatewayToBridge => "GatewayToBridge",
      Direction::BridgeToClient => "BridgeToClient",
      Direction::ClientToBridge => "ClientToBridge",
    },
    r.surface
  );
  let inner_tag = inv
    .enums
    .get(&request_inner_name)
    .or_else(|| inv.enums.get(&response_inner_name))
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

fn classify_outer_variant(variant: &WireVariant, direction: Direction, inv: &Inventory) -> EntryCategory {
  let Some(PayloadType::Named(payload)) = variant.payload.as_ref() else {
    return EntryCategory::Skip;
  };
  let Some(set) = inv.markers.get(payload) else {
    return EntryCategory::Skip;
  };
  if set.has(MarkerKind::Command, direction) {
    EntryCategory::Command
  } else if set.has(MarkerKind::Event, direction) {
    EntryCategory::Event
  } else {
    EntryCategory::Skip
  }
}
