//! Walks `crates/lib/src/` and builds an `Inventory` of wire-protocol
//! structural pieces across both transports:
//!
//! - **Gateway** (Bluetooth, msgpack+gzip): `BridgeToGatewayMsgData` /
//!   `GatewayToBridgeMsgData`.
//! - **Client** (local WebSocket, JSON): `BridgeToClientMsgData` /
//!   `ClientToBridgeMsgData`.
//!
//! Discovers top-level enums, inner enums, marker trait impls (inferred
//! from `#[derive(BridgeEnum)]` per-variant tags + parent ident's
//! direction prefix, plus standalone `#[derive(WireEvent/...)]` derives
//! keyed off `#[wire(<Direction>, ...)]`), and typed-request declarations
//! (`#[derive(WireRequest)]` keyed off `#[wire_request(...)]`).
//!
//! The plan layer groups results by `Protocol` and emits per-protocol
//! per-language helper files.

use std::collections::{BTreeSet, HashMap};

use anyhow::{Context, Result};
use syn::{Attribute, Fields, GenericArgument, Item, ItemEnum, ItemStruct, Meta, PathArguments, Type, Variant};

pub const BRIDGE_TO_GATEWAY: &str = "BridgeToGatewayMsgData";
pub const GATEWAY_TO_BRIDGE: &str = "GatewayToBridgeMsgData";
pub const BRIDGE_TO_CLIENT: &str = "BridgeToClientMsgData";
pub const CLIENT_TO_BRIDGE: &str = "ClientToBridgeMsgData";

/// Wire-direction tag - one per recognized parent-ident prefix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Direction {
  BridgeToGateway,
  GatewayToBridge,
  BridgeToClient,
  ClientToBridge,
}

impl Direction {
  pub fn from_str(s: &str) -> Option<Self> {
    match s {
      "BridgeToGateway" => Some(Self::BridgeToGateway),
      "GatewayToBridge" => Some(Self::GatewayToBridge),
      "BridgeToClient" => Some(Self::BridgeToClient),
      "ClientToBridge" => Some(Self::ClientToBridge),
      _ => None,
    }
  }

  pub fn from_parent_ident(ident: &str) -> Option<Self> {
    if ident.starts_with("BridgeToGateway") {
      Some(Self::BridgeToGateway)
    } else if ident.starts_with("GatewayToBridge") {
      Some(Self::GatewayToBridge)
    } else if ident.starts_with("BridgeToClient") {
      Some(Self::BridgeToClient)
    } else if ident.starts_with("ClientToBridge") {
      Some(Self::ClientToBridge)
    } else {
      None
    }
  }

  pub fn wire_data_name(self) -> &'static str {
    match self {
      Self::BridgeToGateway => BRIDGE_TO_GATEWAY,
      Self::GatewayToBridge => GATEWAY_TO_BRIDGE,
      Self::BridgeToClient => BRIDGE_TO_CLIENT,
      Self::ClientToBridge => CLIENT_TO_BRIDGE,
    }
  }

  /// Opposite direction in the same protocol family. Used for typed
  /// requests where the response arrives on the opposite-direction
  /// wire.
  pub fn opposite(self) -> Self {
    match self {
      Self::BridgeToGateway => Self::GatewayToBridge,
      Self::GatewayToBridge => Self::BridgeToGateway,
      Self::BridgeToClient => Self::ClientToBridge,
      Self::ClientToBridge => Self::BridgeToClient,
    }
  }
}

/// Coarse-grained protocol family. Each protocol owns one pair of
/// `Direction`s.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Protocol {
  Gateway,
  Client,
}

impl Protocol {
  pub fn of(direction: Direction) -> Self {
    match direction {
      Direction::BridgeToGateway | Direction::GatewayToBridge => Self::Gateway,
      Direction::BridgeToClient | Direction::ClientToBridge => Self::Client,
    }
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MarkerKind {
  Event,
  Command,
  Unicast,
}

#[derive(Debug, Clone)]
pub struct WireVariant {
  pub name: String,
  /// Single-field tuple-variant payload. `None` for unit variants AND
  /// for struct-shaped variants - the latter are exposed only at the
  /// parent enum level because per-language type-paths to them differ
  /// enough that codegen for the inner field set isn't worth the
  /// surface.
  pub payload: Option<PayloadType>,
  /// True for `Foo { ... }` named-field variants. Outbound codegen
  /// skips these because constructing the variant requires per-field
  /// args and per-language struct shapes that the dispatch layer
  /// doesn't model.
  pub is_struct: bool,
  /// Per-variant `#[bridge_*]` tag. Lets codegen pick the right wire
  /// `meta.kind` per variant inside an inner enum that mixes events
  /// with commands. `None` for outer wire enums (no per-variant tag).
  pub tag: Option<VariantTag>,
  /// `///` doc on the variant, joined by newline. ts-rs drops these
  /// from the generated `.d.ts`, but the docs emitter reads them from
  /// source here so per-method prose survives.
  pub docs: Option<String>,
}

/// Per-variant tag inferred from `#[bridge_event]` / `#[bridge_command]`
/// / `#[bridge_request]` / `#[bridge_response]` attributes on inner
/// enum variants. Drives per-variant outbound `meta.kind` selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VariantTag {
  Event,
  Command,
  Request,
  Response,
}

/// Semantic categorization of a single-tuple variant payload.
/// Per-language emitters translate these to their native types.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PayloadType {
  /// Named user type (struct or enum). Carries the bare ident
  /// (last path segment).
  Named(String),
  /// `Vec<u8>` - translates to per-language bytes type.
  Bytes,
  /// `serde_json::Value` - translates to per-language unstructured-json type.
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
      Self::Named(n) => match n.as_str() {
        // Disambiguate from Foundation.Notification.
        "Notification" => "BridgethingSchema.Notification".to_string(),
        _ => n.clone(),
      },
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
  /// outer wire enums). Defaults to `"type"` if the enum isn't tagged.
  pub tag_field: String,
  /// Adjacent-tagged content field name (serde `content = "..."`), when
  /// the enum carries payloads. Lets the docs emitter render an accurate
  /// TS discriminated union.
  pub content_field: Option<String>,
  /// `///` doc on the enum container. For a surface's inner enum this
  /// is the human description of the whole surface.
  pub docs: Option<String>,
}

/// A named DTO struct (payload, reply, or a `shared/` type) captured so
/// the docs emitter can expand a method's payload shape field-by-field.
/// Not consumed by any language emitter - docs only.
#[derive(Debug, Clone)]
pub struct StructDef {
  pub docs: Option<String>,
  pub fields: Vec<FieldDef>,
}

#[derive(Debug, Clone)]
pub struct FieldDef {
  /// camelCase wire name (matches the JSON/`.d.ts` field).
  pub name: String,
  /// Rendered TS-facing type string (e.g. `Track[]`, `string`).
  pub ty: String,
  /// True when the Rust type is `Option<T>` (optional on the wire).
  pub optional: bool,
  /// Base named type this field references, for cross-linking and
  /// transitive type collection. `None` for scalars/bytes.
  pub type_ref: Option<String>,
  pub docs: Option<String>,
}

/// Markers attached to one named type, with the wire direction each
/// marker applies to. A type may carry multiple markers across multiple
/// directions (e.g. `ForwardMessage` is `WireEvent<W>` for three wires).
#[derive(Debug, Clone, Default)]
pub struct MarkerSet {
  pub entries: Vec<(MarkerKind, Direction)>,
}

impl MarkerSet {
  pub fn has(&self, kind: MarkerKind, direction: Direction) -> bool {
    self.entries.iter().any(|(k, d)| *k == kind && *d == direction)
  }
}

#[derive(Debug)]
pub struct Inventory {
  pub wire_enums: HashMap<String, EnumDef>,
  pub enums: HashMap<String, EnumDef>,
  /// Named DTO structs by name. Docs-only; the language emitters lean on
  /// ts-rs/typeshare for the actual type bindings.
  pub structs: HashMap<String, StructDef>,
  pub markers: HashMap<String, MarkerSet>,
  pub typed_requests: Vec<TypedRequest>,
  /// camelCase names of every struct field whose Rust type is `Uuid`.
  /// Per-language codecs use this to bridge the on-wire representation
  /// (msgpack 16-byte `bin` on the gateway, JSON hyphenated string on
  /// the local websocket) and the SDK-surface UUID type.
  pub uuid_field_names: BTreeSet<String>,
}

/// A single typed-request declaration, captured in structured form.
/// Codegen reads these to emit typed query methods and typed-handle
/// inbound dispatch in each per-language SDK.
#[derive(Debug, Clone)]
pub struct TypedRequest {
  pub request: String,
  /// Outbound direction: the direction the request enters. Response
  /// arrives on `direction.opposite()`.
  pub direction: Direction,
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
  let mut structs: HashMap<String, StructDef> = HashMap::new();
  let mut markers: HashMap<String, MarkerSet> = HashMap::new();
  let mut typed_requests: Vec<TypedRequest> = Vec::new();
  let mut uuid_field_names: BTreeSet<String> = BTreeSet::new();

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
      &mut structs,
      &mut markers,
      &mut typed_requests,
      &mut uuid_field_names,
    );
  }

  Ok(Inventory {
    wire_enums,
    enums,
    structs,
    markers,
    typed_requests,
    uuid_field_names,
  })
}

fn walk_items(
  items: &[Item],
  wire_enums: &mut HashMap<String, EnumDef>,
  enums: &mut HashMap<String, EnumDef>,
  structs: &mut HashMap<String, StructDef>,
  markers: &mut HashMap<String, MarkerSet>,
  typed_requests: &mut Vec<TypedRequest>,
  uuid_field_names: &mut BTreeSet<String>,
) {
  for item in items {
    match item {
      Item::Enum(en) => {
        let def = collect_enum(en);
        let name = def.name.clone();
        if matches!(
          name.as_str(),
          BRIDGE_TO_GATEWAY | GATEWAY_TO_BRIDGE | BRIDGE_TO_CLIENT | CLIENT_TO_BRIDGE
        ) {
          wire_enums.insert(name.clone(), def);
        } else {
          enums.insert(name.clone(), def);
        }
        // Standalone marker derives can appear on enums too
        // (e.g. `ForwardMessage`).
        for (kind, dir) in standalone_markers(&en.attrs) {
          markers.entry(name.clone()).or_default().entries.push((kind, dir));
        }
        // BridgeEnum-derived enums infer their parent-level marker from
        // per-variant `#[bridge_*]` tags. Direction comes from the parent
        // ident prefix.
        if has_derive(&en.attrs, "BridgeEnum")
          && let Some(direction) = Direction::from_parent_ident(&name)
        {
          let variants: Vec<&Variant> = en.variants.iter().collect();
          for kind in infer_bridge_enum_markers(&variants) {
            markers.entry(name.clone()).or_default().entries.push((kind, direction));
          }
        }
      }
      Item::Struct(s) => {
        let name = s.ident.to_string();
        for (kind, dir) in standalone_markers(&s.attrs) {
          markers.entry(name.clone()).or_default().entries.push((kind, dir));
        }
        if has_derive(&s.attrs, "WireRequest")
          && let Some(req) = parse_wire_request_attr(s)
        {
          typed_requests.push(req);
        }
        collect_uuid_field_names(s, uuid_field_names);
        structs.insert(name.clone(), collect_struct(s));
      }
      Item::Mod(m) => {
        if let Some((_, sub_items)) = &m.content {
          walk_items(
            sub_items,
            wire_enums,
            enums,
            structs,
            markers,
            typed_requests,
            uuid_field_names,
          );
        }
      }
      _ => {}
    }
  }
}

/// Returns the markers declared via standalone derives on this item:
/// `WireEvent`, `WireCommand`, `WireUnicast`, each paired with one or
/// more directions read from the `#[wire(<Direction>, ...)]` attribute
/// on the same item.
fn standalone_markers(attrs: &[Attribute]) -> Vec<(MarkerKind, Direction)> {
  let mut kinds: Vec<MarkerKind> = Vec::new();
  for attr in attrs {
    if !attr.path().is_ident("derive") {
      continue;
    }
    let _ = attr.parse_nested_meta(|meta| {
      if let Some(seg) = meta.path.segments.last() {
        match seg.ident.to_string().as_str() {
          "WireEvent" => kinds.push(MarkerKind::Event),
          "WireCommand" => kinds.push(MarkerKind::Command),
          "WireUnicast" => kinds.push(MarkerKind::Unicast),
          _ => {}
        }
      }
      Ok(())
    });
  }
  if kinds.is_empty() {
    return Vec::new();
  }
  let directions = parse_wire_directions(attrs);
  kinds
    .into_iter()
    .flat_map(|kind| directions.iter().map(move |dir| (kind, *dir)))
    .collect()
}

fn parse_wire_directions(attrs: &[Attribute]) -> Vec<Direction> {
  let mut directions = Vec::new();
  for attr in attrs {
    if !attr.path().is_ident("wire") {
      continue;
    }
    let _ = attr.parse_nested_meta(|meta| {
      if let Some(seg) = meta.path.segments.last()
        && let Some(dir) = Direction::from_str(&seg.ident.to_string())
      {
        directions.push(dir);
      }
      Ok(())
    });
  }
  directions
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
/// tagged `#[bridge_event]` contributes the Event marker;
/// `#[bridge_command]` contributes Command. Request and Response tags
/// don't contribute parent-level markers (typed requests route through
/// `WireRequest` on the request payload type; responses go through
/// `respond_to` and don't need a marker).
fn infer_bridge_enum_markers(variants: &[&Variant]) -> Vec<MarkerKind> {
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
    out.push(MarkerKind::Event);
  }
  if has_command {
    out.push(MarkerKind::Command);
  }
  out
}

/// Parse a `#[wire_request(...)]` attribute off a struct decorated with
/// `#[derive(WireRequest)]`. Format:
///
/// ```text
/// direction = <Ident>,
/// surface = <Ident>,
/// request_variant = <Ident>,
/// response = <TypePath>,
/// response_variant = <Ident>,
/// [error = <TypePath>,
///  error_variant = <Ident>,]
/// ```
fn parse_wire_request_attr(s: &ItemStruct) -> Option<TypedRequest> {
  let attr = s.attrs.iter().find(|a| a.path().is_ident("wire_request"))?;
  let mut direction: Option<Direction> = None;
  let mut surface: Option<String> = None;
  let mut request_variant: Option<String> = None;
  let mut response: Option<String> = None;
  let mut response_variant: Option<String> = None;
  let mut error: Option<String> = None;
  let mut error_variant: Option<String> = None;

  let _ = attr.parse_nested_meta(|meta| {
    if meta.path.is_ident("direction") {
      let id: syn::Ident = meta.value()?.parse()?;
      direction = Direction::from_str(&id.to_string());
    } else if meta.path.is_ident("surface") {
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
    direction: direction?,
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
      tag: variant_tag(&v.attrs),
      docs: doc_string(&v.attrs),
    })
    .collect();
  let tag_field = serde_tag_field(&en.attrs).unwrap_or_else(|| "type".to_string());
  EnumDef {
    name: en.ident.to_string(),
    variants,
    tag_field,
    content_field: serde_content_field(&en.attrs),
    docs: doc_string(&en.attrs),
  }
}

fn collect_struct(s: &ItemStruct) -> StructDef {
  let fields = match &s.fields {
    Fields::Named(named) => named
      .named
      .iter()
      .filter_map(|f| {
        let ident = f.ident.as_ref()?;
        let (ty, optional, type_ref) = render_ts_type(&f.ty);
        Some(FieldDef {
          name: snake_to_camel(&ident.to_string()),
          ty,
          optional,
          type_ref,
          docs: doc_string(&f.attrs),
        })
      })
      .collect(),
    _ => Vec::new(),
  };
  StructDef {
    docs: doc_string(&s.attrs),
    fields,
  }
}

/// Join a Rust item's `///` doc lines (each a `#[doc = "..."]` attribute)
/// into one string. Strips the single leading space rustdoc inserts,
/// preserves paragraph breaks, and returns `None` when there is no doc.
fn doc_string(attrs: &[Attribute]) -> Option<String> {
  let mut lines: Vec<String> = Vec::new();
  for attr in attrs {
    if !attr.path().is_ident("doc") {
      continue;
    }
    if let Meta::NameValue(nv) = &attr.meta
      && let syn::Expr::Lit(syn::ExprLit {
        lit: syn::Lit::Str(s), ..
      }) = &nv.value
    {
      lines.push(s.value().strip_prefix(' ').map(str::to_string).unwrap_or_else(|| s.value()));
    }
  }
  let joined = lines.join("\n").trim().to_string();
  (!joined.is_empty()).then_some(joined)
}

/// Render a Rust field type to a TS-facing string plus the base named
/// type it references (for cross-linking and transitive collection).
/// Returns `(display, is_optional, type_ref)`. `Option<T>` sets the
/// optional flag and unwraps to `T`.
fn render_ts_type(ty: &Type) -> (String, bool, Option<String>) {
  let Type::Path(p) = ty else {
    return (quote::ToTokens::to_token_stream(ty).to_string(), false, None);
  };
  let Some(seg) = p.path.segments.last() else {
    return ("unknown".to_string(), false, None);
  };
  let name = seg.ident.to_string();
  match name.as_str() {
    "Option" => {
      if let Some(inner) = first_generic(seg) {
        let (t, _, r) = render_ts_type(inner);
        (t, true, r)
      } else {
        ("unknown".to_string(), true, None)
      }
    }
    "Box" => first_generic(seg).map(render_ts_type).unwrap_or(("unknown".to_string(), false, None)),
    "Vec" => match first_generic(seg) {
      Some(inner) if is_u8(inner) => ("number[]".to_string(), false, None),
      Some(inner) => {
        let (t, _, r) = render_ts_type(inner);
        (format!("{t}[]"), false, r)
      }
      None => ("unknown[]".to_string(), false, None),
    },
    "HashMap" | "BTreeMap" => {
      let mut args = match &seg.arguments {
        PathArguments::AngleBracketed(a) => a.args.iter().filter_map(|a| match a {
          GenericArgument::Type(t) => Some(t),
          _ => None,
        }),
        _ => return ("Record<string, unknown>".to_string(), false, None),
      };
      let key = args.next().map(|t| render_ts_type(t).0).unwrap_or_else(|| "string".to_string());
      let val = args.next().map(render_ts_type);
      let val_display = val.as_ref().map(|v| v.0.clone()).unwrap_or_else(|| "unknown".to_string());
      let val_ref = val.and_then(|v| v.2);
      (format!("Record<{key}, {val_display}>"), false, val_ref)
    }
    "String" | "str" => ("string".to_string(), false, None),
    "Uuid" => ("string".to_string(), false, None),
    "bool" => ("boolean".to_string(), false, None),
    "u8" | "u16" | "u32" | "u64" | "usize" | "i8" | "i16" | "i32" | "i64" | "isize" | "f32" | "f64" => {
      ("number".to_string(), false, None)
    }
    "Value" => ("unknown".to_string(), false, None),
    other => (other.to_string(), false, Some(other.to_string())),
  }
}

fn is_u8(ty: &Type) -> bool {
  matches!(ty, Type::Path(p) if p.path.segments.last().is_some_and(|s| s.ident == "u8"))
}

fn first_generic(seg: &syn::PathSegment) -> Option<&Type> {
  if let PathArguments::AngleBracketed(args) = &seg.arguments
    && let Some(GenericArgument::Type(inner)) = args.args.first()
  {
    Some(inner)
  } else {
    None
  }
}

fn variant_tag(attrs: &[Attribute]) -> Option<VariantTag> {
  for attr in attrs {
    let path = attr.path();
    if path.is_ident("bridge_event") {
      return Some(VariantTag::Event);
    }
    if path.is_ident("bridge_command") {
      return Some(VariantTag::Command);
    }
    if path.is_ident("bridge_request") {
      return Some(VariantTag::Request);
    }
    if path.is_ident("bridge_response") {
      return Some(VariantTag::Response);
    }
  }
  None
}

fn serde_tag_field(attrs: &[syn::Attribute]) -> Option<String> {
  serde_str_meta(attrs, "tag")
}

fn serde_content_field(attrs: &[syn::Attribute]) -> Option<String> {
  serde_str_meta(attrs, "content")
}

fn serde_str_meta(attrs: &[syn::Attribute], key: &str) -> Option<String> {
  for attr in attrs {
    if !attr.path().is_ident("serde") {
      continue;
    }
    let Ok(nested) = attr.parse_args_with(syn::punctuated::Punctuated::<Meta, syn::Token![,]>::parse_terminated) else {
      continue;
    };
    for meta in nested {
      if let Meta::NameValue(nv) = meta
        && nv.path.is_ident(key)
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

/// Walk a struct's named fields and add the camelCase form of every
/// `Uuid`-typed field to `out`. Wrappers like `Option<Uuid>` are
/// recognized; anything else is skipped.
fn collect_uuid_field_names(s: &ItemStruct, out: &mut BTreeSet<String>) {
  let Fields::Named(named) = &s.fields else {
    return;
  };
  for field in &named.named {
    let Some(ident) = &field.ident else { continue };
    if !is_uuid_type(&field.ty) {
      continue;
    }
    out.insert(snake_to_camel(&ident.to_string()));
  }
}

fn is_uuid_type(ty: &Type) -> bool {
  let Type::Path(p) = ty else { return false };
  let Some(seg) = p.path.segments.last() else {
    return false;
  };
  let name = seg.ident.to_string();
  if name == "Uuid" {
    return true;
  }
  if name == "Option"
    && let PathArguments::AngleBracketed(args) = &seg.arguments
    && let Some(GenericArgument::Type(inner)) = args.args.first()
  {
    return is_uuid_type(inner);
  }
  false
}

fn snake_to_camel(s: &str) -> String {
  let mut out = String::with_capacity(s.len());
  let mut upper_next = false;
  for ch in s.chars() {
    if ch == '_' {
      upper_next = true;
    } else if upper_next {
      out.extend(ch.to_uppercase());
      upper_next = false;
    } else {
      out.push(ch);
    }
  }
  out
}

fn payload_type(ty: &Type) -> PayloadType {
  let Type::Path(p) = ty else {
    return PayloadType::Named(quote::ToTokens::to_token_stream(ty).to_string());
  };
  let Some(seg) = p.path.segments.last() else {
    return PayloadType::Named("_".to_string());
  };
  let name = seg.ident.to_string();
  if name == "Box"
    && let PathArguments::AngleBracketed(args) = &seg.arguments
    && let Some(GenericArgument::Type(inner)) = args.args.first()
  {
    return payload_type(inner);
  }
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
