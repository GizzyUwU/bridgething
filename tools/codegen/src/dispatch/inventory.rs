//! Walks `crates/lib/src/` and builds an `Inventory` of the wire
//! protocol's structural pieces: top-level enums, inner enums, marker
//! trait impls (inferred from `#[derive(BridgeEnum)]` per-variant tags
//! plus standalone `#[derive(BridgeEvent/...)]` derives), and typed-
//! request declarations (read from `#[gateway_request(...)]` /
//! `#[bridge_request(...)]` attributes paired with their derives).
//! The plan layer consumes this to produce the per-language emit plan.
//!
//! Codegen used to walk hand-written `impl Trait for Type {}` blocks in
//! `crates/lib/src/gateway/marker.rs` and regex-parse `impl_*_request!`
//! macro tokens. Both have been replaced by proc-macro derives, so this
//! file now reads attributes via `syn` directly. The shape of `Inventory`
//! is unchanged — the plan layer is decoupled from how the inventory is
//! discovered.

use std::collections::HashMap;

use anyhow::{Context, Result};
use syn::{Attribute, Fields, GenericArgument, Item, ItemEnum, ItemStruct, Meta, PathArguments, Type, Variant};

pub const BRIDGE_TO_GATEWAY: &str = "BridgeToGatewayMsgData";
pub const GATEWAY_TO_BRIDGE: &str = "GatewayToBridgeMsgData";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MarkerKind {
  BridgeEvent,
  GatewayEvent,
  BridgeCommand,
  GatewayCommand,
  BridgeUnicast,
  GatewayUnicast,
}

impl MarkerKind {
  fn from_path(name: &str) -> Option<Self> {
    match name {
      "BridgeEvent" => Some(Self::BridgeEvent),
      "GatewayEvent" => Some(Self::GatewayEvent),
      "BridgeCommand" => Some(Self::BridgeCommand),
      "GatewayCommand" => Some(Self::GatewayCommand),
      "BridgeUnicast" => Some(Self::BridgeUnicast),
      "GatewayUnicast" => Some(Self::GatewayUnicast),
      _ => None,
    }
  }
}

#[derive(Debug, Clone)]
pub struct WireVariant {
  pub name: String,
  /// Single-field tuple-variant payload. `None` for unit variants
  /// AND for struct-shaped variants — the latter are exposed only at
  /// the parent enum level because per-language type-paths to them
  /// differ enough that codegen for the inner field set isn't worth
  /// the surface.
  pub payload: Option<PayloadType>,
  /// True for `Foo { ... }` named-field variants. Outbound codegen
  /// skips these because constructing the variant requires per-field
  /// args and per-language struct shapes that the dispatch layer
  /// doesn't model.
  pub is_struct: bool,
}

/// Semantic categorization of a single-tuple variant payload.
/// Per-language emitters translate these to their native types.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PayloadType {
  /// Named user type (struct or enum). Carries the bare ident
  /// (last path segment).
  Named(String),
  /// `Vec<u8>` — translates to per-language bytes type.
  Bytes,
  /// `serde_json::Value` — translates to per-language unstructured-json type.
  JsonValue,
  /// Plain `String`.
  StringScalar,
}

impl PayloadType {
  pub fn ts(&self) -> String {
    match self {
      Self::Named(n) => n.clone(),
      Self::Bytes => "Uint8Array".to_string(),
      Self::JsonValue => "unknown".to_string(),
      Self::StringScalar => "string".to_string(),
    }
  }
  pub fn kotlin(&self) -> String {
    match self {
      Self::Named(n) => n.clone(),
      Self::Bytes => "ByteArray".to_string(),
      Self::JsonValue => "Value".to_string(),
      Self::StringScalar => "String".to_string(),
    }
  }
  pub fn swift(&self) -> String {
    match self {
      Self::Named(n) => n.clone(),
      Self::Bytes => "Data".to_string(),
      Self::JsonValue => "Value".to_string(),
      Self::StringScalar => "String".to_string(),
    }
  }
}

#[derive(Debug, Clone)]
pub struct EnumDef {
  pub name: String,
  pub variants: Vec<WireVariant>,
  /// Adjacent-tagged discriminator field name (e.g. `"event"` for most
  /// inner enums, `"encoding"` for `ForwardMessage`, `"type"` for the
  /// outer wire enums and `GatewayError`). Defaults to `"type"` if the
  /// enum isn't tagged.
  pub tag_field: String,
}

#[derive(Debug)]
pub struct Inventory {
  pub wire_enums: HashMap<String, EnumDef>,
  pub enums: HashMap<String, EnumDef>,
  pub markers: HashMap<String, Vec<MarkerKind>>,
  pub gateway_requests: Vec<TypedRequest>,
  pub bridge_requests: Vec<TypedRequest>,
}

/// A single typed-request declaration, captured in structured form.
/// Codegen reads these to emit typed query methods and typed-handle
/// inbound dispatch in each per-language SDK.
#[derive(Debug, Clone)]
pub struct TypedRequest {
  pub request: String,
  pub surface: String,
  pub request_variant: String,
  pub request_takes_payload: bool,
  pub response: String,
  pub response_variant: String,
  pub error: Option<String>,
  pub error_variant: Option<String>,
}

pub fn inventory(lib_src: &str) -> Result<Inventory> {
  let mut wire_enums = HashMap::new();
  let mut enums = HashMap::new();
  let mut markers: HashMap<String, Vec<MarkerKind>> = HashMap::new();
  let mut gateway_requests: Vec<TypedRequest> = Vec::new();
  let mut bridge_requests: Vec<TypedRequest> = Vec::new();

  for entry in walkdir::WalkDir::new(lib_src) {
    let entry = entry.context("walk lib_src")?;
    let path = entry.path();
    if !entry.file_type().is_file() {
      continue;
    }
    if path.extension().and_then(|s| s.to_str()) != Some("rs") {
      continue;
    }
    let src = std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let parsed = match syn::parse_file(&src) {
      Ok(p) => p,
      Err(e) => {
        eprintln!("    warning: dispatch: failed to parse {}: {e}", path.display());
        continue;
      }
    };
    walk_items(
      &parsed.items,
      &mut wire_enums,
      &mut enums,
      &mut markers,
      &mut gateway_requests,
      &mut bridge_requests,
    );
  }

  for kinds in markers.values_mut() {
    kinds.sort_by_key(|k| *k as u8);
    kinds.dedup();
  }

  Ok(Inventory {
    wire_enums,
    enums,
    markers,
    gateway_requests,
    bridge_requests,
  })
}

fn walk_items(
  items: &[Item],
  wire_enums: &mut HashMap<String, EnumDef>,
  enums: &mut HashMap<String, EnumDef>,
  markers: &mut HashMap<String, Vec<MarkerKind>>,
  gateway_requests: &mut Vec<TypedRequest>,
  bridge_requests: &mut Vec<TypedRequest>,
) {
  for item in items {
    match item {
      Item::Enum(en) => {
        let def = collect_enum(en);
        let name = def.name.clone();
        if name == BRIDGE_TO_GATEWAY || name == GATEWAY_TO_BRIDGE {
          wire_enums.insert(name.clone(), def);
        } else {
          enums.insert(name.clone(), def);
        }
        // Standalone marker derives can appear on enums too
        // (e.g. `ForwardMessage`).
        for kind in standalone_marker_derives(&en.attrs) {
          markers.entry(name.clone()).or_default().push(kind);
        }
        // BridgeEnum-derived enums infer their parent-level marker from
        // per-variant `#[bridge_*]` tags. Direction comes from the parent
        // ident prefix.
        if has_derive(&en.attrs, "BridgeEnum") {
          let variants: Vec<&Variant> = en.variants.iter().collect();
          for kind in infer_bridge_enum_markers(&name, &variants) {
            markers.entry(name.clone()).or_default().push(kind);
          }
        }
      }
      Item::Struct(s) => {
        // Structs only contribute markers (top-level outer-wire variant
        // payloads like `BridgeThingMeta`) and typed-request declarations.
        let name = s.ident.to_string();
        for kind in standalone_marker_derives(&s.attrs) {
          markers.entry(name.clone()).or_default().push(kind);
        }
        if has_derive(&s.attrs, "GatewayRequest")
          && let Some(req) = parse_request_attr(s, "gateway_request")
        {
          gateway_requests.push(req);
        }
        if has_derive(&s.attrs, "BridgeRequest")
          && let Some(req) = parse_request_attr(s, "bridge_request")
        {
          bridge_requests.push(req);
        }
      }
      Item::Mod(m) => {
        if let Some((_, sub_items)) = &m.content {
          walk_items(sub_items, wire_enums, enums, markers, gateway_requests, bridge_requests);
        }
      }
      _ => {}
    }
  }
}

/// Returns the markers declared via standalone derives on this item:
/// `BridgeEvent`, `GatewayEvent`, `BridgeCommand`, `GatewayCommand`.
/// `BridgeUnicast` / `GatewayUnicast` are still hand-written impls today
/// (no current users); they don't go through this path.
fn standalone_marker_derives(attrs: &[Attribute]) -> Vec<MarkerKind> {
  let mut out = Vec::new();
  for attr in attrs {
    if !attr.path().is_ident("derive") {
      continue;
    }
    let _ = attr.parse_nested_meta(|meta| {
      if let Some(seg) = meta.path.segments.last()
        && let Some(kind) = MarkerKind::from_path(&seg.ident.to_string())
      {
        out.push(kind);
      }
      Ok(())
    });
  }
  out
}

fn has_derive(attrs: &[Attribute], name: &str) -> bool {
  for attr in attrs {
    if !attr.path().is_ident("derive") {
      continue;
    }
    let mut found = false;
    let _ = attr.parse_nested_meta(|meta| {
      if let Some(seg) = meta.path.segments.last()
        && seg.ident == name
      {
        found = true;
      }
      Ok(())
    });
    if found {
      return true;
    }
  }
  false
}

/// For an enum with `#[derive(BridgeEnum)]`, infer the parent-level
/// marker traits from the per-variant `#[bridge_*]` tags. A variant
/// tagged `#[bridge_event]` contributes the direction's Event marker;
/// `#[bridge_command]` contributes Command. Request and Response tags
/// don't contribute parent-level markers (typed requests route through
/// `BridgeRequest`/`GatewayRequest` traits on the request payload type;
/// responses go through `respond_to` and don't need a marker).
fn infer_bridge_enum_markers(parent_name: &str, variants: &[&Variant]) -> Vec<MarkerKind> {
  let direction_is_bridge_to_gateway = parent_name.starts_with("BridgeToGateway");
  let direction_is_gateway_to_bridge = parent_name.starts_with("GatewayToBridge");
  if !direction_is_bridge_to_gateway && !direction_is_gateway_to_bridge {
    return Vec::new();
  }
  let mut has_event = false;
  let mut has_command = false;
  for v in variants {
    for attr in &v.attrs {
      if attr.path().is_ident("bridge_event") {
        has_event = true;
      } else if attr.path().is_ident("bridge_command") {
        has_command = true;
      }
    }
  }
  let mut out = Vec::new();
  if has_event {
    out.push(if direction_is_bridge_to_gateway {
      MarkerKind::BridgeEvent
    } else {
      MarkerKind::GatewayEvent
    });
  }
  if has_command {
    out.push(if direction_is_bridge_to_gateway {
      MarkerKind::BridgeCommand
    } else {
      MarkerKind::GatewayCommand
    });
  }
  out
}

/// Parse a `#[gateway_request(...)]` / `#[bridge_request(...)]` attribute
/// off a struct decorated with the matching derive. Format:
///
/// ```text
/// surface = <Ident>,
/// request_variant = <Ident>,
/// response = <TypePath>,
/// response_variant = <Ident>,
/// [error = <TypePath>,
///  error_variant = <Ident>,]
/// ```
///
/// `request_takes_payload` is determined from the struct's shape:
/// `Fields::Unit` → unit-variant; anything else → tuple-variant.
fn parse_request_attr(s: &ItemStruct, attr_name: &str) -> Option<TypedRequest> {
  let attr = s.attrs.iter().find(|a| a.path().is_ident(attr_name))?;
  let mut surface: Option<String> = None;
  let mut request_variant: Option<String> = None;
  let mut response: Option<String> = None;
  let mut response_variant: Option<String> = None;
  let mut error: Option<String> = None;
  let mut error_variant: Option<String> = None;

  let _ = attr.parse_nested_meta(|meta| {
    if meta.path.is_ident("surface") {
      let v: syn::Path = meta.value()?.parse()?;
      surface = v.segments.last().map(|s| s.ident.to_string());
    } else if meta.path.is_ident("request_variant") {
      let v: syn::Ident = meta.value()?.parse()?;
      request_variant = Some(v.to_string());
    } else if meta.path.is_ident("response") {
      let v: syn::Path = meta.value()?.parse()?;
      response = v.segments.last().map(|s| s.ident.to_string());
    } else if meta.path.is_ident("response_variant") {
      let v: syn::Ident = meta.value()?.parse()?;
      response_variant = Some(v.to_string());
    } else if meta.path.is_ident("error") {
      let v: syn::Path = meta.value()?.parse()?;
      error = v.segments.last().map(|s| s.ident.to_string());
    } else if meta.path.is_ident("error_variant") {
      let v: syn::Ident = meta.value()?.parse()?;
      error_variant = Some(v.to_string());
    } else {
      return Err(meta.error("unknown key"));
    }
    Ok(())
  });

  let request_takes_payload = !matches!(s.fields, Fields::Unit);
  Some(TypedRequest {
    request: s.ident.to_string(),
    surface: surface?,
    request_variant: request_variant?,
    request_takes_payload,
    response: response?,
    response_variant: response_variant?,
    error,
    error_variant,
  })
}

fn collect_enum(en: &ItemEnum) -> EnumDef {
  let variants = en
    .variants
    .iter()
    .map(|v| WireVariant {
      name: v.ident.to_string(),
      payload: variant_single_payload(&v.fields),
      is_struct: matches!(v.fields, Fields::Named(_)),
    })
    .collect();
  let tag_field = serde_tag_field(&en.attrs).unwrap_or_else(|| "type".to_string());
  EnumDef {
    name: en.ident.to_string(),
    variants,
    tag_field,
  }
}

fn serde_tag_field(attrs: &[syn::Attribute]) -> Option<String> {
  for attr in attrs {
    if !attr.path().is_ident("serde") {
      continue;
    }
    let Ok(nested) = attr.parse_args_with(syn::punctuated::Punctuated::<Meta, syn::Token![,]>::parse_terminated) else {
      continue;
    };
    for meta in nested {
      if let Meta::NameValue(nv) = meta
        && nv.path.is_ident("tag")
        && let syn::Expr::Lit(lit) = nv.value
        && let syn::Lit::Str(s) = lit.lit
      {
        return Some(s.value());
      }
    }
  }
  None
}

fn variant_single_payload(fields: &Fields) -> Option<PayloadType> {
  match fields {
    Fields::Unit => None,
    Fields::Unnamed(unnamed) if unnamed.unnamed.len() == 1 => Some(payload_type(&unnamed.unnamed[0].ty)),
    _ => None,
  }
}

fn payload_type(ty: &Type) -> PayloadType {
  let Type::Path(p) = ty else {
    return PayloadType::Named(quote::ToTokens::to_token_stream(ty).to_string());
  };
  let Some(seg) = p.path.segments.last() else {
    return PayloadType::Named("_".to_string());
  };
  let name = seg.ident.to_string();
  if name == "Vec"
    && let PathArguments::AngleBracketed(args) = &seg.arguments
    && let Some(GenericArgument::Type(Type::Path(inner))) = args.args.first()
    && inner.path.segments.last().is_some_and(|s| s.ident == "u8")
  {
    return PayloadType::Bytes;
  }
  if name == "Value" {
    return PayloadType::JsonValue;
  }
  if name == "String" {
    return PayloadType::StringScalar;
  }
  PayloadType::Named(name)
}
