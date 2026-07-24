pub use librespot_protocol as upstream;

pub mod custom {
  include!(concat!(env!("OUT_DIR"), "/custom_protos/mod.rs"));
}
