//! Emits the SDK reference IR as JSON, consumed by bridgething.com to
//! render `/docs`. Scoped to the client protocol - the surface a webapp
//! developer sees (`client.<surface>.<method>`). Reuses the same `Plan`
//! and `surfaces()` derivation the TypeScript client emitter uses, so
//! the documented shape can never drift from the generated SDK, and
//! harvests the `///` docs the inventory captured (including per-variant
//! docs that ts-rs drops from the `.d.ts`).

use std::{
  collections::{BTreeMap, VecDeque},
  path::Path,
};

use anyhow::{Context, Result};
use serde::Serialize;

use super::{
  inventory::{Inventory, PayloadType},
  plan::{EntryCategory, Plan, Surface, TypedRequestEntry, rename_camel, surfaces},
};

#[derive(Serialize)]
struct DocsOutput {
  /// Workspace/package version this IR was generated from.
  version: String,
  surfaces: Vec<SurfaceDoc>,
  /// Closed dictionary of every named type reachable from a method
  /// payload, reply, or error, expanded field-by-field or variant-by-variant.
  types: BTreeMap<String, TypeDoc>,
}

#[derive(Serialize)]
struct SurfaceDoc {
  /// camelCase property on the client (e.g. `player`).
  name: String,
  /// Human title (e.g. `Player`).
  title: String,
  #[serde(skip_serializing_if = "Option::is_none")]
  description: Option<String>,
  /// Daemon-pushed events the webapp subscribes to (`onXxx`).
  events: Vec<MethodDoc>,
  /// Typed requests the webapp sends and awaits a reply for.
  requests: Vec<MethodDoc>,
  /// Fire-and-forget commands the webapp sends.
  commands: Vec<MethodDoc>,
  /// Typed requests the daemon sends and the webapp answers (`onXxx`).
  handlers: Vec<MethodDoc>,
}

#[derive(Serialize)]
struct MethodDoc {
  /// SDK method name (`onSnapshot`, `stateGet`, `play`).
  method: String,
  #[serde(skip_serializing_if = "Option::is_none")]
  description: Option<String>,
  /// Payload type display (event/command payload, or request input).
  #[serde(skip_serializing_if = "Option::is_none")]
  payload: Option<String>,
  /// Named type the payload links to, when it is a user type.
  #[serde(skip_serializing_if = "Option::is_none")]
  payload_ref: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  response: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  error: Option<String>,
  /// The `onXxx` subscription form of a request's reply (the SDK exposes
  /// the response variant as an event too). Attached to the request so
  /// the reply is documented alongside it, not as a stray event.
  #[serde(skip_serializing_if = "Option::is_none")]
  response_event: Option<String>,
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum TypeDoc {
  Struct {
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    fields: Vec<FieldDoc>,
  },
  Enum {
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    /// Adjacent-tagged discriminator field (for payload-bearing enums).
    #[serde(skip_serializing_if = "Option::is_none")]
    tag: Option<String>,
    /// Adjacent-tagged content field (for payload-bearing enums).
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
    variants: Vec<VariantDoc>,
  },
}

#[derive(Serialize)]
struct FieldDoc {
  name: String,
  #[serde(rename = "type")]
  ty: String,
  #[serde(skip_serializing_if = "std::ops::Not::not")]
  optional: bool,
  #[serde(skip_serializing_if = "Option::is_none")]
  type_ref: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  description: Option<String>,
}

#[derive(Serialize)]
struct VariantDoc {
  name: String,
  #[serde(skip_serializing_if = "Option::is_none")]
  payload: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  payload_ref: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  description: Option<String>,
}

/// Display + link-target for a single-tuple payload.
fn payload_parts(p: &PayloadType) -> (String, Option<String>) {
  match p {
    PayloadType::Named(n) => (n.clone(), Some(n.clone())),
    other => (other.ts(), None),
  }
}

/// Owned inner-enum name of a dispatch entry's outer payload.
fn inner_enum_name(entry: &Option<super::plan::DispatchEntry>) -> Option<String> {
  match entry.as_ref()?.outer_payload.as_ref()? {
    PayloadType::Named(n) => Some(n.clone()),
    _ => None,
  }
}

/// Look up a variant's `///` doc off its source inner enum.
fn variant_doc<'a>(inv: &'a Inventory, enum_name: Option<&str>, variant: &str) -> Option<&'a str> {
  let en = inv.enums.get(enum_name?)?;
  en.variants
    .iter()
    .find(|v| v.name == variant)
    .and_then(|v| v.docs.as_deref())
}

fn title_case(prop: &str) -> String {
  let mut chars = prop.chars();
  match chars.next() {
    Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
    None => String::new(),
  }
}

pub fn emit_docs_json(plan: &Plan, inv: &Inventory, out_path: &str) -> Result<()> {
  let mut refs: VecDeque<String> = VecDeque::new();
  let note = |name: Option<&str>, refs: &mut VecDeque<String>| {
    if let Some(n) = name {
      refs.push_back(n.to_string());
    }
  };

  let surface_docs: Vec<SurfaceDoc> = surfaces(plan).iter().map(|s| build_surface(s, inv, &mut refs)).collect();

  // Transitively expand every referenced named type into a closed
  // dictionary. A worklist BFS keyed on a visited set; scalars and
  // types the inventory never captured are simply left as bare
  // references the site renders without expansion.
  let mut types: BTreeMap<String, TypeDoc> = BTreeMap::new();
  while let Some(name) = refs.pop_front() {
    if types.contains_key(&name) {
      continue;
    }
    if let Some(st) = inv.structs.get(&name) {
      let fields = st
        .fields
        .iter()
        .map(|f| {
          note(f.type_ref.as_deref(), &mut refs);
          FieldDoc {
            name: f.name.clone(),
            ty: f.ty.clone(),
            optional: f.optional,
            type_ref: f.type_ref.clone(),
            description: f.docs.clone(),
          }
        })
        .collect();
      types.insert(
        name.clone(),
        TypeDoc::Struct {
          description: st.docs.clone(),
          fields,
        },
      );
    } else if let Some(en) = inv.enums.get(&name) {
      let variants = en
        .variants
        .iter()
        .map(|v| {
          let (payload, payload_ref) = match &v.payload {
            Some(p) => {
              let (d, r) = payload_parts(p);
              (Some(d), r)
            }
            None => (None, None),
          };
          note(payload_ref.as_deref(), &mut refs);
          VariantDoc {
            name: v.name.clone(),
            payload,
            payload_ref,
            description: v.docs.clone(),
          }
        })
        .collect();
      let has_payload = en.variants.iter().any(|v| v.payload.is_some());
      types.insert(
        name.clone(),
        TypeDoc::Enum {
          description: en.docs.clone(),
          tag: has_payload.then(|| en.tag_field.clone()),
          content: if has_payload { en.content_field.clone() } else { None },
          variants,
        },
      );
    }
    // else: not a captured struct/enum (scalar alias, external) - the
    // site renders the bare name.
  }

  let output = DocsOutput {
    version: env!("CARGO_PKG_VERSION").to_string(),
    surfaces: surface_docs,
    types,
  };

  let json = serde_json::to_string_pretty(&output).context("serialize docs json")?;
  if let Some(parent) = Path::new(out_path).parent() {
    std::fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
  }
  std::fs::write(out_path, format!("{json}\n")).with_context(|| format!("write {out_path}"))?;
  Ok(())
}

fn build_surface(s: &Surface, inv: &Inventory, refs: &mut VecDeque<String>) -> SurfaceDoc {
  let inbound_inner = inner_enum_name(&s.inbound);
  let outbound_inner = inner_enum_name(&s.outbound);

  // Surface description: the inbound container doc reads as "daemon ->
  // webapp <surface>"; fall back to the outbound one.
  let description = inbound_inner
    .as_deref()
    .and_then(|n| inv.enums.get(n))
    .and_then(|e| e.docs.clone())
    .or_else(|| {
      outbound_inner
        .as_deref()
        .and_then(|n| inv.enums.get(n))
        .and_then(|e| e.docs.clone())
    });

  // Genuine `#[bridge_event]` variants only. The `#[bridge_response]`
  // variants (category None) are the reply half of a typed request and
  // are documented on the request, not as stray events.
  let events = s
    .inbound_event_variants()
    .iter()
    .filter(|iv| matches!(iv.category, Some(EntryCategory::Event)))
    .map(|iv| {
      let (payload, payload_ref) = match &iv.payload {
        Some(p) => {
          let (d, r) = payload_parts(p);
          (Some(d), r)
        }
        None => (None, None),
      };
      if let Some(r) = &payload_ref {
        refs.push_back(r.clone());
      }
      MethodDoc {
        method: format!("on{}", iv.variant),
        description: variant_doc(inv, inbound_inner.as_deref(), &iv.variant).map(str::to_string),
        payload,
        payload_ref,
        response: None,
        error: None,
        response_event: None,
      }
    })
    .collect();

  let commands = s
    .outbound_send_variants()
    .iter()
    .map(|iv| {
      let (payload, payload_ref) = match &iv.payload {
        Some(p) => {
          let (d, r) = payload_parts(p);
          (Some(d), r)
        }
        None => (None, None),
      };
      if let Some(r) = &payload_ref {
        refs.push_back(r.clone());
      }
      MethodDoc {
        method: rename_camel(&iv.variant),
        description: variant_doc(inv, outbound_inner.as_deref(), &iv.variant).map(str::to_string),
        payload,
        payload_ref,
        response: None,
        error: None,
        response_event: None,
      }
    })
    .collect();

  let requests = s
    .outbound_queries
    .iter()
    .map(|r| {
      build_request(
        r,
        inv,
        refs,
        rename_camel(&r.request_variant_pascal()),
        Some(format!("on{}", r.response_variant_pascal())),
      )
    })
    .collect();

  let handlers = s
    .inbound_requests
    .iter()
    .map(|r| build_request(r, inv, refs, format!("on{}", r.request_variant_pascal()), None))
    .collect();

  SurfaceDoc {
    name: s.prop.clone(),
    title: title_case(&s.prop),
    description,
    events,
    requests,
    commands,
    handlers,
  }
}

fn build_request(
  r: &TypedRequestEntry,
  inv: &Inventory,
  refs: &mut VecDeque<String>,
  method: String,
  response_event: Option<String>,
) -> MethodDoc {
  refs.push_back(r.response.clone());
  if let Some(e) = &r.error {
    refs.push_back(e.clone());
  }
  let (payload, payload_ref) = if r.request_takes_payload {
    refs.push_back(r.request.clone());
    (Some(r.request.clone()), Some(r.request.clone()))
  } else {
    (None, None)
  };
  MethodDoc {
    method,
    description: inv.structs.get(&r.request).and_then(|st| st.docs.clone()),
    payload,
    payload_ref,
    response: Some(r.response.clone()),
    error: r.error.clone(),
    response_event,
  }
}
