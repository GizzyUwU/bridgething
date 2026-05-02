//! Zero-argument marker-trait derives for top-level outer-wire variant
//! payload types. Each derive emits `impl <Trait> for Self {}` against
//! the corresponding marker trait in `libbridgething::gateway`.
//!
//! Used for payload types that appear directly as variants on
//! `BridgeToGatewayMsgData` / `GatewayToBridgeMsgData` without an
//! intermediate inner enum (e.g. `BridgeThingMeta`, `GatewayMeta`,
//! `ForwardMessage`, `NowPlayingUpdate`). For inner enums that already
//! use `#[derive(BridgeEnum)]`, the marker impl lands on the per-bucket
//! sibling enum instead of the parent — these standalone derives are
//! only for the non-enum-wrapped case.

use proc_macro_crate::{FoundCrate, crate_name};
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::quote;
use syn::{DeriveInput, Ident};

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

pub(crate) fn expand(ast: &DeriveInput, marker: &str) -> syn::Result<TokenStream2> {
  let name = &ast.ident;
  let (impl_generics, ty_generics, where_clause) = ast.generics.split_for_impl();
  let lib = lib_crate_path();
  let marker_ident = Ident::new(marker, Span::call_site());
  Ok(quote! {
    impl #impl_generics #lib::gateway::#marker_ident for #name #ty_generics #where_clause {}
  })
}
