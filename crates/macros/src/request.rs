//! `#[derive(GatewayRequest)]` and `#[derive(BridgeRequest)]` — replace
//! the macro_rules `impl_*_request!` pair with proc-macro derives keyed
//! off a single attribute.
//!
//! Usage on a request payload type:
//!
//! ```ignore
//! #[derive(..., GatewayRequest)]
//! #[gateway_request(
//!     surface = Webapp,
//!     request_variant = SwitchTo,
//!     response = WebappActive,
//!     response_variant = Switched,
//!     error = WebappError,
//!     error_variant = WebappError,
//! )]
//! pub struct WebappSwitchTo { pub name: String }
//! ```
//!
//! What gets emitted:
//! 1. `impl GatewayRequest for <Self>` (or `BridgeRequest`) with
//!    `extract`, `encode_response`, `encode_domain_error`.
//! 2. `From<Self> for <OuterMsgData>` for the outbound wire wrapping.
//! 3. A `const _: () = { … }` block referencing the hidden response-
//!    marker module emitted by `BridgeEnum` on the cross-direction
//!    inner enum. This block fails to compile if the response variant
//!    doesn't exist there, isn't tagged `#[bridge_response]`, or
//!    carries a payload type other than the one declared here. The
//!    same assertion runs for the (optional) error variant.
//!
//! Direction conventions (`GatewayRequest` is companion → daemon, so
//! its response lives on the daemon → companion direction's inner enum,
//! and vice versa) are baked into the `Direction` enum below.
//!
//! Tuple vs unit request variants are resolved from the `Self` type's
//! shape: a unit struct (no fields) emits a unit-variant constructor;
//! anything else emits a tuple-variant constructor that takes the
//! whole `Self`. With-error vs without-error is decided by whether
//! the attribute carries `error` / `error_variant` keys; `without` uses
//! `Infallible` as the domain-error type.

use proc_macro_crate::{FoundCrate, crate_name};
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::{format_ident, quote};
use syn::{Attribute, Data, DeriveInput, Fields, Ident, Type, spanned::Spanned};

#[derive(Copy, Clone, PartialEq, Eq)]
pub(crate) enum Direction {
  /// `GatewayRequest`: companion → daemon. Response on the daemon →
  /// companion side (`BridgeToGateway*`).
  Gateway,
  /// `BridgeRequest`: daemon → companion. Response on the companion
  /// → daemon side (`GatewayToBridge*`).
  Bridge,
}

impl Direction {
  fn trait_ident(self) -> Ident {
    match self {
      Self::Gateway => format_ident!("GatewayRequest"),
      Self::Bridge => format_ident!("BridgeRequest"),
    }
  }

  /// The wire data type the request *enters*: for `GatewayRequest`,
  /// the companion → daemon `GatewayToBridgeMsgData`; for
  /// `BridgeRequest`, the daemon → companion `BridgeToGatewayMsgData`.
  fn outbound_wire(self) -> Ident {
    match self {
      Self::Gateway => format_ident!("GatewayToBridgeMsgData"),
      Self::Bridge => format_ident!("BridgeToGatewayMsgData"),
    }
  }

  /// The inner enum holding the *request* variant (the same direction
  /// as `outbound_wire`), parameterized by surface name.
  fn outbound_inner(self, surface: &Ident) -> Ident {
    match self {
      Self::Gateway => format_ident!("GatewayToBridge{}Msg", surface),
      Self::Bridge => format_ident!("BridgeToGateway{}Msg", surface),
    }
  }

  /// The wire data type the response *enters* (opposite of `outbound_wire`).
  fn response_wire(self) -> Ident {
    match self {
      Self::Gateway => format_ident!("BridgeToGatewayMsgData"),
      Self::Bridge => format_ident!("GatewayToBridgeMsgData"),
    }
  }

  /// The inner enum holding the *response* and *error* variants.
  fn response_inner(self, surface: &Ident) -> Ident {
    match self {
      Self::Gateway => format_ident!("BridgeToGateway{}Msg", surface),
      Self::Bridge => format_ident!("GatewayToBridge{}Msg", surface),
    }
  }

  fn attr_name(self) -> &'static str {
    match self {
      Self::Gateway => "gateway_request",
      Self::Bridge => "bridge_request",
    }
  }
}

struct RequestAttr {
  surface: Ident,
  request_variant: Ident,
  response: Type,
  response_variant: Ident,
  error: Option<Type>,
  error_variant: Option<Ident>,
}

fn parse_attr(attrs: &[Attribute], attr_name: &str, parent_span: Span) -> syn::Result<RequestAttr> {
  for attr in attrs {
    if !attr.path().is_ident(attr_name) {
      continue;
    }
    let mut surface: Option<Ident> = None;
    let mut request_variant: Option<Ident> = None;
    let mut response: Option<Type> = None;
    let mut response_variant: Option<Ident> = None;
    let mut error: Option<Type> = None;
    let mut error_variant: Option<Ident> = None;

    attr.parse_nested_meta(|meta| {
      if meta.path.is_ident("surface") {
        surface = Some(meta.value()?.parse()?);
      } else if meta.path.is_ident("request_variant") {
        request_variant = Some(meta.value()?.parse()?);
      } else if meta.path.is_ident("response") {
        response = Some(meta.value()?.parse()?);
      } else if meta.path.is_ident("response_variant") {
        response_variant = Some(meta.value()?.parse()?);
      } else if meta.path.is_ident("error") {
        error = Some(meta.value()?.parse()?);
      } else if meta.path.is_ident("error_variant") {
        error_variant = Some(meta.value()?.parse()?);
      } else {
        return Err(meta.error(format!("unsupported {} key", attr_name)));
      }
      Ok(())
    })?;

    let surface =
      surface.ok_or_else(|| syn::Error::new(attr.span(), format!("{} missing `surface = …`", attr_name)))?;
    let request_variant = request_variant
      .ok_or_else(|| syn::Error::new(attr.span(), format!("{} missing `request_variant = …`", attr_name)))?;
    let response =
      response.ok_or_else(|| syn::Error::new(attr.span(), format!("{} missing `response = …`", attr_name)))?;
    let response_variant = response_variant
      .ok_or_else(|| syn::Error::new(attr.span(), format!("{} missing `response_variant = …`", attr_name)))?;

    if error.is_some() != error_variant.is_some() {
      return Err(syn::Error::new(
        attr.span(),
        format!("{} requires both or neither of `error` and `error_variant`", attr_name),
      ));
    }

    return Ok(RequestAttr {
      surface,
      request_variant,
      response,
      response_variant,
      error,
      error_variant,
    });
  }
  Err(syn::Error::new(
    parent_span,
    format!("missing #[{}(…)] attribute", attr_name),
  ))
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

/// Returns true if the request payload type is a unit struct (no
/// fields). Unit structs map to unit-shaped request variants; anything
/// else maps to tuple variants that carry the whole `Self`.
fn is_unit_struct(ast: &DeriveInput) -> syn::Result<bool> {
  match &ast.data {
    Data::Struct(s) => match &s.fields {
      Fields::Unit => Ok(true),
      _ => Ok(false),
    },
    _ => Err(syn::Error::new(
      ast.span(),
      "GatewayRequest / BridgeRequest only supports structs (the request payload type)",
    )),
  }
}

pub(crate) fn expand(ast: &DeriveInput, direction: Direction) -> syn::Result<TokenStream2> {
  let attr = parse_attr(&ast.attrs, direction.attr_name(), ast.span())?;
  let unit_request = is_unit_struct(ast)?;

  let req_ty = &ast.ident;
  let lib = lib_crate_path();
  let trait_ident = direction.trait_ident();
  let outbound_wire = direction.outbound_wire();
  let outbound_inner = direction.outbound_inner(&attr.surface);
  let response_wire = direction.response_wire();
  let response_inner = direction.response_inner(&attr.surface);
  let response_marker_mod = format_ident!("__{}_responses", response_inner);

  let request_variant = &attr.request_variant;
  let response_ty = &attr.response;
  let response_variant = &attr.response_variant;

  // The outer wire data wraps the inner enum as
  // `Self::<Surface>(inner)`; the `surface` ident is also the variant
  // ident on the outer wire data.
  let outbound_inner_outer = &attr.surface;

  // `From<Self> for <outbound_wire>`: how the request wraps into the
  // outer wire data.
  let from_impl = if unit_request {
    quote! {
      impl ::core::convert::From<#req_ty> for #lib::gateway::#outbound_wire {
        fn from(_: #req_ty) -> Self {
          #lib::gateway::#outbound_wire::#outbound_inner_outer(
            #lib::gateway::#outbound_inner::#request_variant
          )
        }
      }
    }
  } else {
    quote! {
      impl ::core::convert::From<#req_ty> for #lib::gateway::#outbound_wire {
        fn from(payload: #req_ty) -> Self {
          #lib::gateway::#outbound_wire::#outbound_inner_outer(
            #lib::gateway::#outbound_inner::#request_variant(payload)
          )
        }
      }
    }
  };

  let (domain_error_ty, extract_arms_error, encode_domain_error_body, error_assertion) =
    if let (Some(err_ty), Some(err_variant)) = (&attr.error, &attr.error_variant) {
      let extract = quote! {
        #lib::gateway::#response_wire::#outbound_inner_outer(
          #lib::gateway::#response_inner::#err_variant(e),
        ) => ::core::result::Result::Err(#lib::gateway::RequestError::Domain(e)),
      };
      let encode = quote! {
        #lib::gateway::#response_wire::#outbound_inner_outer(
          #lib::gateway::#response_inner::#err_variant(err),
        )
      };
      let assertion = quote! {
        let _ = #lib::gateway::#response_marker_mod::#err_variant(
          ::core::marker::PhantomData::<<#req_ty as #lib::gateway::#trait_ident>::DomainError>,
        );
      };
      (quote! { #err_ty }, extract, quote! { #encode }, assertion)
    } else {
      (
        quote! { ::core::convert::Infallible },
        quote! {},
        quote! { match err {} },
        quote! {},
      )
    };

  let response_assertion = quote! {
    let _ = #lib::gateway::#response_marker_mod::#response_variant(
      ::core::marker::PhantomData::<<#req_ty as #lib::gateway::#trait_ident>::Response>,
    );
  };

  let trait_impl = quote! {
    impl #lib::gateway::#trait_ident for #req_ty {
      type Response = #response_ty;
      type DomainError = #domain_error_ty;

      fn extract(
        data: #lib::gateway::#response_wire,
      ) -> ::core::result::Result<Self::Response, #lib::gateway::RequestError<Self::DomainError>> {
        match data {
          #lib::gateway::#response_wire::#outbound_inner_outer(
            #lib::gateway::#response_inner::#response_variant(v),
          ) => ::core::result::Result::Ok(v),
          #extract_arms_error
          #lib::gateway::#response_wire::Error(e) => {
            ::core::result::Result::Err(#lib::gateway::RequestError::Protocol(e))
          }
          _ => ::core::result::Result::Err(#lib::gateway::RequestError::ResponseMismatch),
        }
      }

      fn encode_response(v: Self::Response) -> #lib::gateway::#response_wire {
        #lib::gateway::#response_wire::#outbound_inner_outer(
          #lib::gateway::#response_inner::#response_variant(v),
        )
      }

      fn encode_domain_error(err: Self::DomainError) -> #lib::gateway::#response_wire {
        #encode_domain_error_body
      }
    }
  };

  let cross_assertion = quote! {
    const _: () = {
      #response_assertion
      #error_assertion
    };
  };

  Ok(quote! {
    #trait_impl
    #from_impl
    #cross_assertion
  })
}
