use libbridgething::NluSlots;
use nlu::SlotValue;
use serde::de::DeserializeOwned;
use serde_json::Value;

pub fn apply(slots: &[SlotValue]) -> NluSlots {
  let mut out = NluSlots::default();
  for slot in slots {
    let value = slot.value.as_str();
    match slot.name.as_str() {
      "target" => out.target = Some(value.to_owned()),
      "playlist" => out.playlist = Some(value.to_owned()),
      "genre" => out.genre = Some(value.to_owned()),
      "mood" => out.mood = Some(value.to_owned()),
      "era" => out.era = Some(value.to_owned()),
      "webapp_name" => out.webapp_name = Some(value.to_owned()),
      "preset" => out.preset = Some(value.to_owned()),
      "target_type" => out.target_type = wire_enum(&camel(value)),
      "popularity_filter" => out.popularity_filter = wire_enum(&camel(value)),
      "scope" => out.scope = wire_enum(&camel(value)),
      "view" => out.view = wire_enum(&camel(value)),
      "repeat_mode" => out.repeat_mode = wire_enum(&camel(value)),
      "direction" => out.direction = wire_enum(&camel(value)),
      "amount" => out.amount = wire_enum(&camel(value)),
      "phone_action" => out.phone_action = wire_enum(&camel(value)),
      "speed" => out.speed = wire_enum(value),
      "enabled" => out.enabled = parse_bool(value),
      "mute" => out.mute = parse_bool(value),
      "count" => out.count = value.parse().ok(),
      "position" => out.position = value.parse().ok(),
      "level" => out.level = value.parse().ok(),
      "seconds" => out.seconds = value.parse().ok(),
      _ => {}
    }
  }
  out
}

pub fn camel(token: &str) -> String {
  let mut parts = token.split('_');
  let Some(head) = parts.next() else {
    return token.to_owned();
  };
  let mut out = head.to_owned();
  for part in parts {
    let mut characters = part.chars();
    if let Some(first) = characters.next() {
      out.extend(first.to_uppercase());
      out.push_str(characters.as_str());
    }
  }
  out
}

pub fn parse_bool(token: &str) -> Option<bool> {
  match token.to_lowercase().as_str() {
    "true" => Some(true),
    "false" => Some(false),
    _ => None,
  }
}

fn wire_enum<T: DeserializeOwned>(token: &str) -> Option<T> {
  serde_json::from_value(Value::String(token.to_owned())).ok()
}
