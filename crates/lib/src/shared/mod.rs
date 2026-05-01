mod bluetooth;
mod forward;
mod now_playing;
mod peer;
mod player;
mod priority;
mod system;
mod webapp;

pub use bluetooth::*;
pub use forward::*;
pub use now_playing::*;
pub use peer::*;
pub use player::*;
pub use priority::*;
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
