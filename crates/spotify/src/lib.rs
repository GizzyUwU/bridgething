//! credit to the librespot project

pub mod aplogin;
pub mod auth;
pub mod client;
pub mod dealer;
pub mod error;
pub mod http;
pub mod httpx;
pub mod logging;
pub mod model;
pub mod proto;
pub mod spclient;
pub mod store;
pub mod transport;
pub mod util;

pub use error::{Error, Result};

uniffi::setup_scaffolding!();
