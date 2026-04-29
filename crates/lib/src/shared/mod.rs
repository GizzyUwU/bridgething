mod bluetooth;
mod forward;
mod player;
mod system;
mod webapp;

pub use bluetooth::*;
pub use forward::*;
pub use player::*;
pub use system::*;
pub use webapp::*;

pub fn to_slug(value: &str) -> String {
  value
    .trim()
    .replace(' ', "_")
    .chars()
    .filter(|c| c.is_alphanumeric())
    .collect::<String>()
}
