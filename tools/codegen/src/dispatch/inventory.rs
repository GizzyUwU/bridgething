//! Walks `crates/lib/src/` and builds an `Inventory` of the wire
//! protocol's structural pieces: top-level enums, inner enums, marker
//! impls, and typed-request macro invocations. The plan layer consumes
//! this to produce the per-language emit plan.

use std::collections::HashMap;

use anyhow::{Context, Result};
use syn::{Fields, GenericArgument, Item, ItemEnum, ItemImpl, Meta, PathArguments, Type};

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

/// A single `impl_gateway_request!` or `impl_bridge_request!` invocation,
/// captured in structured form. Codegen reads these to emit typed
/// query methods and typed-handle inbound dispatch.
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
        if def.name == BRIDGE_TO_GATEWAY || def.name == GATEWAY_TO_BRIDGE {
          wire_enums.insert(def.name.clone(), def);
        } else {
          enums.insert(def.name.clone(), def);
        }
      }
      Item::Impl(im) => {
        if let Some((kind, target)) = parse_marker_impl(im) {
          markers.entry(target).or_default().push(kind);
        }
      }
      Item::Macro(m) => {
        let path = m
          .mac
          .path
          .segments
          .last()
          .map(|s| s.ident.to_string())
          .unwrap_or_default();
        if path == "impl_gateway_request" {
          if let Some(req) = parse_typed_request(&m.mac.tokens.to_string()) {
            gateway_requests.push(req);
          }
        } else if path == "impl_bridge_request"
          && let Some(req) = parse_typed_request(&m.mac.tokens.to_string())
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

/// Parse a structured `impl_*_request!` macro body into a `TypedRequest`.
/// Format produced by the macro signature in `crates/lib/src/gateway/request.rs`:
///
/// ```text
/// request: <Type>,
/// surface: <Ident>,
/// request_variant: <Ident> | <Ident>(_),
/// response: <Type>,
/// response_variant: <Ident>(_),
/// [error: <Type>,
///  error_variant: <Ident>(_),]
/// ```
fn parse_typed_request(body: &str) -> Option<TypedRequest> {
  let request = extract_field(body, "request")?;
  let surface = extract_field(body, "surface")?;
  let (request_variant, request_takes_payload) = extract_variant_with_kind(body, "request_variant")?;
  let response = extract_field(body, "response")?;
  let (response_variant, _) = extract_variant_with_kind(body, "response_variant")?;
  let error = extract_field(body, "error");
  let error_variant = extract_variant_with_kind(body, "error_variant").map(|(v, _)| v);
  Some(TypedRequest {
    request,
    surface,
    request_variant,
    request_takes_payload,
    response,
    response_variant,
    error,
    error_variant,
  })
}

fn extract_field(body: &str, name: &str) -> Option<String> {
  let pattern = regex::Regex::new(&format!(r"\b{}\s*:\s*([A-Za-z_][A-Za-z0-9_]*)", regex::escape(name))).ok()?;
  pattern.captures(body).map(|c| c[1].to_string())
}

/// Extract a `<name>: <Variant>` or `<name>: <Variant>(_)` capture and
/// report whether parens were present (true = tuple/payload variant,
/// false = unit variant).
fn extract_variant_with_kind(body: &str, name: &str) -> Option<(String, bool)> {
  let pattern = regex::Regex::new(&format!(
    r"\b{}\s*:\s*([A-Za-z_][A-Za-z0-9_]*)(\s*\(\s*_\s*\))?",
    regex::escape(name)
  ))
  .ok()?;
  let cap = pattern.captures(body)?;
  let variant = cap.get(1)?.as_str().to_string();
  let takes_payload = cap.get(2).is_some();
  Some((variant, takes_payload))
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

fn parse_marker_impl(im: &ItemImpl) -> Option<(MarkerKind, String)> {
  let (_, trait_path, _) = im.trait_.as_ref()?;
  let trait_name = trait_path.segments.last()?.ident.to_string();
  let kind = MarkerKind::from_path(&trait_name)?;
  let target = match im.self_ty.as_ref() {
    Type::Path(p) => p.path.segments.last()?.ident.to_string(),
    _ => return None,
  };
  Some((kind, target))
}
