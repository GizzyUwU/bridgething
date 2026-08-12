pub const SURFACE_NAMES: [&str; 22] = [
  "ADD_TO_PLAYLIST",
  "ADD_TO_QUEUE",
  "CANCEL_INTERACTION",
  "HELP",
  "MORE_LIKE_THIS",
  "NEXT",
  "OPEN_WEBAPP",
  "PAUSE",
  "PHONE_ACTION",
  "PLAY",
  "PRESET_PLAY",
  "PRESET_SAVE",
  "PREVIOUS",
  "SEARCH",
  "SEEK_RELATIVE",
  "SET_DISCOVERABLE",
  "SET_PLAYBACK_SPEED",
  "SET_REPEAT",
  "SET_SHUFFLE",
  "SET_VOLUME",
  "SHOW_VIEW",
  "THUMBS_UP",
];

pub const NO_INTENT: &str = "NO_INTENT";
pub const CLARIFY: &str = "CLARIFY";

pub fn name_at(index: usize) -> Option<&'static str> {
  SURFACE_NAMES.get(index).copied()
}

pub fn contains(name: &str) -> bool {
  SURFACE_NAMES.contains(&name)
}
