mod bluetooth;
mod player;
mod system;

pub use bluetooth::*;
pub use player::*;
pub use system::*;

pub fn to_slug(value: &str) -> String {
  value
    .trim()
    .replace(' ', "_")
    .chars()
    .filter(|c| c.is_alphanumeric())
    .collect::<String>()
}
