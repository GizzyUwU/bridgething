//! `#[derive(BridgeEnum)]` — split a gateway-wire enum into per-category
//! sibling enums and emit the supporting trait infrastructure.
//!
//! Each variant is tagged with exactly one of `#[bridge_event]`,
//! `#[bridge_command]`, `#[bridge_request]`, `#[bridge_response]`. The
//! macro emits, per non-empty bucket:
//!
//! - `<Parent><Bucket>` enum with just that bucket's variants;
//! - `From<<Parent><Bucket>> for <Parent>` (sibling lifts to parent);
//! - `<Parent>::into_<bucket>(self) -> Option<<Parent><Bucket>>` and
//!   `<Parent>::is_<bucket>_variant(&self) -> bool`;
//! - For Event/Command: marker trait impl on the sibling
//!   (`BridgeEvent`/`BridgeCommand` for `BridgeToGateway*` parents,
//!   `GatewayEvent`/`GatewayCommand` for `GatewayToBridge*` parents),
//!   plus `From<Sibling> for <OuterMsgData>` when the parent declares
//!   `#[bridge_enum(into = OuterPath)]`.
//! - For Response: a hidden `__<Parent>_responses` marker module,
//!   carrying one phantom-typed struct per response variant. The
//!   `GatewayRequest`/`BridgeRequest` derives reference these to
//!   compile-time-validate that a declared response variant exists,
//!   is tagged `#[bridge_response]`, and matches the declared payload
//!   type. See `request.rs` for the consumer side.
//!
//! Direction is inferred from the parent ident — `BridgeToGateway*` is
//! daemon-emitted (markers `BridgeEvent`/`BridgeCommand`),
//! `GatewayToBridge*` is companion-emitted (markers
//! `GatewayEvent`/`GatewayCommand`).

use std::collections::BTreeMap;

use proc_macro_crate::{FoundCrate, crate_name};
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::{format_ident, quote};
use syn::{Attribute, Data, DeriveInput, Fields, Ident, Path, Variant, Visibility, spanned::Spanned};

#[derive(Copy, Clone, PartialEq, Eq)]
enum Direction {
  BridgeToGateway,
  GatewayToBridge,
}

impl Direction {
  fn from_ident(ident: &Ident) -> syn::Result<Self> {
    let s = ident.to_string();
    if s.starts_with("BridgeToGateway") {
      Ok(Self::BridgeToGateway)
    } else if s.starts_with("GatewayToBridge") {
      Ok(Self::GatewayToBridge)
    } else {
      Err(syn::Error::new(
        ident.span(),
        "BridgeEnum requires the enum to be named with prefix \
         `BridgeToGateway` (daemon-emitted) or `GatewayToBridge` \
         (companion-emitted) so direction can be inferred",
      ))
    }
  }

  fn event_marker(self) -> Ident {
    match self {
      Self::BridgeToGateway => format_ident!("BridgeEvent"),
      Self::GatewayToBridge => format_ident!("GatewayEvent"),
    }
  }

  fn command_marker(self) -> Ident {
    match self {
      Self::BridgeToGateway => format_ident!("BridgeCommand"),
      Self::GatewayToBridge => format_ident!("GatewayCommand"),
    }
  }
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum Bucket {
  Event,
  Command,
  Request,
  Response,
}

impl Bucket {
  fn suffix(self) -> &'static str {
    match self {
      Self::Event => "Event",
      Self::Command => "Command",
      Self::Request => "Request",
      Self::Response => "Response",
    }
  }

  fn snake(self) -> &'static str {
    match self {
      Self::Event => "event",
      Self::Command => "command",
      Self::Request => "request",
      Self::Response => "response",
    }
  }
}

fn variant_bucket(v: &Variant) -> syn::Result<Bucket> {
  let mut found: Option<(Bucket, Span)> = None;
  for attr in &v.attrs {
    let bucket = if attr.path().is_ident("bridge_event") {
      Some(Bucket::Event)
    } else if attr.path().is_ident("bridge_command") {
      Some(Bucket::Command)
    } else if attr.path().is_ident("bridge_request") {
      Some(Bucket::Request)
    } else if attr.path().is_ident("bridge_response") {
      Some(Bucket::Response)
    } else {
      None
    };
    if let Some(b) = bucket {
      if found.is_some() {
        return Err(syn::Error::new(
          attr.span(),
          "variant must have exactly one of #[bridge_event], #[bridge_command], #[bridge_request], #[bridge_response]",
        ));
      }
      found = Some((b, attr.span()));
    }
  }
  found.map(|(b, _)| b).ok_or_else(|| {
    syn::Error::new(
      v.span(),
      "variant must be tagged with one of #[bridge_event], #[bridge_command], #[bridge_request], #[bridge_response]",
    )
  })
}

fn parse_into_attr(attrs: &[Attribute]) -> syn::Result<Option<Path>> {
  for attr in attrs {
    if !attr.path().is_ident("bridge_enum") {
      continue;
    }
    let mut into: Option<Path> = None;
    attr.parse_nested_meta(|meta| {
      if meta.path.is_ident("into") {
        let value = meta.value()?;
        let path: Path = value.parse()?;
        into = Some(path);
        Ok(())
      } else {
        Err(meta.error("unsupported bridge_enum container attribute"))
      }
    })?;
    if into.is_some() {
      return Ok(into);
    }
  }
  Ok(None)
}

fn lib_crate_path() -> TokenStream2 {
  match crate_name("libbridgething") {
    Ok(FoundCrate::Itself) => quote!(crate),
    Ok(FoundCrate::Name(name)) => {
      let ident = Ident::new(&name, Span::call_site());
      quote!(::#ident)
    }
    Err(_) => quote!(::libbridgething),
  }
}

fn validate_variant_fields(v: &Variant) -> syn::Result<()> {
  match &v.fields {
    Fields::Unit => Ok(()),
    Fields::Unnamed(u) if u.unnamed.len() == 1 => Ok(()),
    _ => Err(syn::Error::new(
      v.span(),
      "BridgeEnum supports only unit and single-tuple variants",
    )),
  }
}

pub(crate) fn expand(ast: &DeriveInput) -> syn::Result<TokenStream2> {
  let Data::Enum(en) = &ast.data else {
    return Err(syn::Error::new(ast.span(), "BridgeEnum only supports enums"));
  };

  let parent = &ast.ident;
  let vis = &ast.vis;
  let direction = Direction::from_ident(parent)?;
  let into_outer = parse_into_attr(&ast.attrs)?;

  let mut grouped: BTreeMap<Bucket, Vec<&Variant>> = BTreeMap::new();
  for v in &en.variants {
    validate_variant_fields(v)?;
    let b = variant_bucket(v)?;
    grouped.entry(b).or_default().push(v);
  }

  let total: usize = grouped.values().map(|v| v.len()).sum();
  let lib_path = lib_crate_path();

  let mut out = TokenStream2::new();
  let mut method_pieces: Vec<TokenStream2> = Vec::new();

  for (&bucket, variants) in &grouped {
    out.extend(emit_sibling_enum(parent, vis, bucket, variants));
    out.extend(emit_from_sibling_for_parent(parent, bucket, variants));

    if bucket == Bucket::Event || bucket == Bucket::Command {
      let marker = match bucket {
        Bucket::Event => direction.event_marker(),
        Bucket::Command => direction.command_marker(),
        _ => unreachable!(),
      };
      out.extend(emit_marker_impl(&lib_path, &marker, parent, bucket));
      if let Some(outer) = &into_outer {
        out.extend(emit_from_sibling_for_outer(parent, bucket, outer));
      }
    }

    let needs_catchall = variants.len() < total;
    method_pieces.push(emit_into_method(parent, bucket, variants, needs_catchall));
    method_pieces.push(emit_is_variant_method(bucket, variants));
  }

  if !method_pieces.is_empty() {
    out.extend(quote! {
      impl #parent {
        #(#method_pieces)*
      }
    });
  }

  if let Some(response_vars) = grouped.get(&Bucket::Response) {
    out.extend(emit_response_marker_module(parent, vis, response_vars));
  }

  Ok(out)
}

fn emit_sibling_enum(parent: &Ident, vis: &Visibility, bucket: Bucket, variants: &[&Variant]) -> TokenStream2 {
  let sibling = format_ident!("{}{}", parent, bucket.suffix());
  let decls = variants.iter().map(|v| {
    let v_ident = &v.ident;
    match &v.fields {
      Fields::Unit => quote! { #v_ident },
      Fields::Unnamed(u) => {
        let ty = &u.unnamed[0].ty;
        quote! { #v_ident(#ty) }
      }
      _ => unreachable!(),
    }
  });
  let doc = format!(
    "{}-tagged subset of [`{}`]. Construct via `<Parent>::into_{}` or directly.",
    bucket.suffix(),
    parent,
    bucket.snake(),
  );
  quote! {
    #[doc = #doc]
    #[derive(::core::fmt::Debug, ::core::clone::Clone)]
    #vis enum #sibling {
      #(#decls),*
    }
  }
}

fn emit_from_sibling_for_parent(parent: &Ident, bucket: Bucket, variants: &[&Variant]) -> TokenStream2 {
  let sibling = format_ident!("{}{}", parent, bucket.suffix());
  let arms = variants.iter().map(|v| {
    let v_ident = &v.ident;
    match &v.fields {
      Fields::Unit => quote! { #sibling::#v_ident => #parent::#v_ident },
      Fields::Unnamed(_) => quote! { #sibling::#v_ident(p) => #parent::#v_ident(p) },
      _ => unreachable!(),
    }
  });
  quote! {
    impl ::core::convert::From<#sibling> for #parent {
      fn from(value: #sibling) -> Self {
        match value {
          #(#arms),*
        }
      }
    }
  }
}

fn emit_from_sibling_for_outer(parent: &Ident, bucket: Bucket, outer: &Path) -> TokenStream2 {
  let sibling = format_ident!("{}{}", parent, bucket.suffix());
  quote! {
    impl ::core::convert::From<#sibling> for #outer {
      fn from(value: #sibling) -> Self {
        let parent: #parent = ::core::convert::From::from(value);
        ::core::convert::From::from(parent)
      }
    }
  }
}

fn emit_marker_impl(lib_path: &TokenStream2, marker: &Ident, parent: &Ident, bucket: Bucket) -> TokenStream2 {
  let sibling = format_ident!("{}{}", parent, bucket.suffix());
  quote! {
    impl #lib_path::gateway::#marker for #sibling {}
  }
}

fn emit_into_method(parent: &Ident, bucket: Bucket, variants: &[&Variant], needs_catchall: bool) -> TokenStream2 {
  let sibling = format_ident!("{}{}", parent, bucket.suffix());
  let method = format_ident!("into_{}", bucket.snake());
  let mut arms: Vec<TokenStream2> = variants
    .iter()
    .map(|v| {
      let v_ident = &v.ident;
      match &v.fields {
        Fields::Unit => quote! {
          Self::#v_ident => ::core::option::Option::Some(#sibling::#v_ident)
        },
        Fields::Unnamed(_) => quote! {
          Self::#v_ident(payload) => ::core::option::Option::Some(#sibling::#v_ident(payload))
        },
        _ => unreachable!(),
      }
    })
    .collect();
  if needs_catchall {
    arms.push(quote! { _ => ::core::option::Option::None });
  }
  let doc = format!(
    "Narrow to the {bucket}-typed sibling. Returns `None` for variants in other buckets.",
    bucket = bucket.snake(),
  );
  quote! {
    #[doc = #doc]
    pub fn #method(self) -> ::core::option::Option<#sibling> {
      match self {
        #(#arms),*
      }
    }
  }
}

fn emit_is_variant_method(bucket: Bucket, variants: &[&Variant]) -> TokenStream2 {
  let method = format_ident!("is_{}_variant", bucket.snake());
  let patterns = variants.iter().map(|v| {
    let v_ident = &v.ident;
    match &v.fields {
      Fields::Unit => quote! { Self::#v_ident },
      Fields::Unnamed(_) => quote! { Self::#v_ident(_) },
      _ => unreachable!(),
    }
  });
  let doc = format!("Returns `true` for variants tagged `#[bridge_{}]`.", bucket.snake());
  quote! {
    #[doc = #doc]
    pub fn #method(&self) -> bool {
      ::core::matches!(self, #(#patterns)|*)
    }
  }
}

fn emit_response_marker_module(parent: &Ident, vis: &Visibility, variants: &[&Variant]) -> TokenStream2 {
  let mod_name = format_ident!("__{}_responses", parent);
  let entries = variants.iter().map(|v| {
    let v_ident = &v.ident;
    match &v.fields {
      Fields::Unit => quote! { pub struct #v_ident; },
      Fields::Unnamed(u) => {
        let ty = &u.unnamed[0].ty;
        quote! { pub struct #v_ident(pub ::core::marker::PhantomData<super::#ty>); }
      }
      _ => unreachable!(),
    }
  });
  let doc = format!(
    "Hidden marker module emitted by `BridgeEnum` for response variants of [`{}`]. \
     `GatewayRequest` / `BridgeRequest` derives reference these structs in a `const _: () = {{ \
     ... }}` block to compile-time-validate that the declared response variant exists, is tagged \
     `#[bridge_response]`, and matches the declared payload type.",
    parent
  );
  quote! {
    #[doc = #doc]
    #[doc(hidden)]
    #[allow(non_snake_case, non_camel_case_types, dead_code)]
    #vis mod #mod_name {
      #(#entries)*
    }
  }
}
