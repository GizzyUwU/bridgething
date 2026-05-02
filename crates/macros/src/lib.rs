//! Proc-macro derives shared across the bridgething workspace.
//!
//! - `#[derive(Csm)]` — iAP2 control-session messages (see `csm.rs`).
//! - `#[derive(BridgeEnum)]` — gateway wire enums; per-bucket sibling
//!   enums + marker impls + cross-enum response-validation modules
//!   (see `bridge_enum.rs`).
//! - `#[derive(GatewayRequest)]` / `#[derive(BridgeRequest)]` — typed
//!   request implementation + cross-enum compile-time validation
//!   (see `request.rs`).
//! - `#[derive(BridgeEvent)]` / `#[derive(GatewayEvent)]` /
//!   `#[derive(BridgeCommand)]` / `#[derive(GatewayCommand)]` — zero-arg
//!   marker derives for top-level outer-wire variant payload types
//!   (see `markers.rs`).

use proc_macro::TokenStream;
use syn::{DeriveInput, parse_macro_input};

mod bridge_enum;
mod csm;
mod markers;
mod request;

#[proc_macro_derive(Csm, attributes(csm))]
pub fn derive_csm(input: TokenStream) -> TokenStream {
  let ast = parse_macro_input!(input as DeriveInput);
  match csm::expand(&ast) {
    Ok(ts) => ts.into(),
    Err(err) => err.to_compile_error().into(),
  }
}

#[proc_macro_derive(
  BridgeEnum,
  attributes(bridge_enum, bridge_event, bridge_command, bridge_request, bridge_response)
)]
pub fn derive_bridge_enum(input: TokenStream) -> TokenStream {
  let ast = parse_macro_input!(input as DeriveInput);
  match bridge_enum::expand(&ast) {
    Ok(ts) => ts.into(),
    Err(err) => err.to_compile_error().into(),
  }
}

#[proc_macro_derive(GatewayRequest, attributes(gateway_request))]
pub fn derive_gateway_request(input: TokenStream) -> TokenStream {
  let ast = parse_macro_input!(input as DeriveInput);
  match request::expand(&ast, request::Direction::Gateway) {
    Ok(ts) => ts.into(),
    Err(err) => err.to_compile_error().into(),
  }
}

#[proc_macro_derive(BridgeRequest, attributes(bridge_request))]
pub fn derive_bridge_request(input: TokenStream) -> TokenStream {
  let ast = parse_macro_input!(input as DeriveInput);
  match request::expand(&ast, request::Direction::Bridge) {
    Ok(ts) => ts.into(),
    Err(err) => err.to_compile_error().into(),
  }
}

#[proc_macro_derive(BridgeEvent)]
pub fn derive_bridge_event(input: TokenStream) -> TokenStream {
  let ast = parse_macro_input!(input as DeriveInput);
  match markers::expand(&ast, "BridgeEvent") {
    Ok(ts) => ts.into(),
    Err(err) => err.to_compile_error().into(),
  }
}

#[proc_macro_derive(GatewayEvent)]
pub fn derive_gateway_event(input: TokenStream) -> TokenStream {
  let ast = parse_macro_input!(input as DeriveInput);
  match markers::expand(&ast, "GatewayEvent") {
    Ok(ts) => ts.into(),
    Err(err) => err.to_compile_error().into(),
  }
}

#[proc_macro_derive(BridgeCommand)]
pub fn derive_bridge_command(input: TokenStream) -> TokenStream {
  let ast = parse_macro_input!(input as DeriveInput);
  match markers::expand(&ast, "BridgeCommand") {
    Ok(ts) => ts.into(),
    Err(err) => err.to_compile_error().into(),
  }
}

#[proc_macro_derive(GatewayCommand)]
pub fn derive_gateway_command(input: TokenStream) -> TokenStream {
  let ast = parse_macro_input!(input as DeriveInput);
  match markers::expand(&ast, "GatewayCommand") {
    Ok(ts) => ts.into(),
    Err(err) => err.to_compile_error().into(),
  }
}

#[proc_macro_derive(BridgeUnicast)]
pub fn derive_bridge_unicast(input: TokenStream) -> TokenStream {
  let ast = parse_macro_input!(input as DeriveInput);
  match markers::expand(&ast, "BridgeUnicast") {
    Ok(ts) => ts.into(),
    Err(err) => err.to_compile_error().into(),
  }
}

#[proc_macro_derive(GatewayUnicast)]
pub fn derive_gateway_unicast(input: TokenStream) -> TokenStream {
  let ast = parse_macro_input!(input as DeriveInput);
  match markers::expand(&ast, "GatewayUnicast") {
    Ok(ts) => ts.into(),
    Err(err) => err.to_compile_error().into(),
  }
}
