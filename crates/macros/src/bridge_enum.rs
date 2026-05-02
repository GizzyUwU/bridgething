//! `#[derive(BridgeEnum)]` — split a gateway-wire enum into its
//! event-only subset.
//!
//! Annotate response and domain-error variants with `#[bridge_response]`.
//! The macro emits a sibling `<Name>Event` enum holding only the
//! unannotated variants plus a `into_event(self) -> Option<<Name>Event>`
//! method on the parent. Daemon-side dispatchers take the narrowed type
//! and the response variants become type-impossible in those handlers.
//!
//! The `<Name>Event` type is internal Rust only — no serde, no ts-rs,
//! no typeshare derives; it never crosses the wire. Only `Debug + Clone`
//! are derived so handlers can log and clone routinely.

use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{Data, DeriveInput, Fields, Variant, spanned::Spanned};

pub(crate) fn expand(ast: &DeriveInput) -> syn::Result<TokenStream2> {
  let Data::Enum(en) = &ast.data else {
    return Err(syn::Error::new(ast.span(), "BridgeEnum only supports enums"));
  };
  let parent = &ast.ident;
  let event_name = format_ident!("{}Event", parent);
  let vis = &ast.vis;

  let mut event_variants = Vec::new();
  let mut event_arms = Vec::new();
  let mut has_response_variants = false;

  for variant in &en.variants {
    if is_response_variant(variant) {
      has_response_variants = true;
      continue;
    }
    let v_ident = &variant.ident;
    match &variant.fields {
      Fields::Unit => {
        event_variants.push(quote! { #v_ident });
        event_arms.push(quote! {
          Self::#v_ident => ::core::option::Option::Some(#event_name::#v_ident)
        });
      }
      Fields::Unnamed(unnamed) if unnamed.unnamed.len() == 1 => {
        let ty = &unnamed.unnamed[0].ty;
        event_variants.push(quote! { #v_ident(#ty) });
        event_arms.push(quote! {
          Self::#v_ident(payload) => ::core::option::Option::Some(#event_name::#v_ident(payload))
        });
      }
      _ => {
        return Err(syn::Error::new(
          variant.span(),
          "BridgeEnum supports only unit and single-tuple variants",
        ));
      }
    }
  }

  let match_body = if has_response_variants {
    quote! {
      match self {
        #(#event_arms),*,
        _ => ::core::option::Option::None,
      }
    }
  } else {
    quote! {
      match self {
        #(#event_arms),*
      }
    }
  };

  Ok(quote! {
    /// Event-only subset of the parent wire enum. Variants tagged
    /// `#[bridge_response]` on the parent are dropped here. Construct
    /// via `<Parent>::into_event(self)`.
    #[derive(::core::fmt::Debug, ::core::clone::Clone)]
    #vis enum #event_name {
      #(#event_variants),*
    }

    impl #parent {
      /// Narrow the wire enum to its event-only subset. Returns
      /// `None` for typed response/error variants — those are
      /// expected to be filtered upstream by the gateway handler's
      /// `is_response_variant` hook before reaching code that needs
      /// the narrowed shape.
      pub fn into_event(self) -> ::core::option::Option<#event_name> {
        #match_body
      }
    }
  })
}

fn is_response_variant(variant: &Variant) -> bool {
  variant.attrs.iter().any(|a| a.path().is_ident("bridge_response"))
}
