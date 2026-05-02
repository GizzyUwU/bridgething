//! Proc-macro derives shared across the bridgething workspace.
//!
//! - `#[derive(Csm)]` — iAP2 control-session messages. See `csm.rs`.
//! - `#[derive(BridgeEnum)]` — gateway wire enums that mix
//!   fire-and-forget event variants with typed-request response/error
//!   variants. Generates a sibling `<Name>Event` enum holding only the
//!   non-response variants plus a `into_event(self) -> Option<<Name>Event>`
//!   conversion, so daemon-side handlers can take the narrowed type and
//!   exhaustively match on events without ever seeing response shapes.

use proc_macro::TokenStream;
use syn::{DeriveInput, parse_macro_input};

mod bridge_enum;
mod csm;

#[proc_macro_derive(Csm, attributes(csm))]
pub fn derive_csm(input: TokenStream) -> TokenStream {
  let ast = parse_macro_input!(input as DeriveInput);
  match csm::expand(&ast) {
    Ok(ts) => ts.into(),
    Err(err) => err.to_compile_error().into(),
  }
}

#[proc_macro_derive(BridgeEnum, attributes(bridge_response))]
pub fn derive_bridge_enum(input: TokenStream) -> TokenStream {
  let ast = parse_macro_input!(input as DeriveInput);
  match bridge_enum::expand(&ast) {
    Ok(ts) => ts.into(),
    Err(err) => err.to_compile_error().into(),
  }
}
