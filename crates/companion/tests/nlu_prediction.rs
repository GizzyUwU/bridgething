use bridgething_companion::voice::{fast_path, intent_catalog};
use libbridgething::{
  NluAlternate, NluAmount, NluDirection, NluPhoneAction, NluPlaybackSpeed, NluPopularityFilter, NluRepeatMode,
  NluResolvedIntent, NluScope, NluSlots, NluTargetType, NluView,
};

fn every_slot_populated() -> NluSlots {
  NluSlots {
    target: Some("black country new road".into()),
    target_type: Some(NluTargetType::Artist),
    playlist: Some("late night".into()),
    genre: Some("jazz".into()),
    mood: Some("chill".into()),
    era: Some("90s".into()),
    popularity_filter: Some(NluPopularityFilter::Top5),
    position: Some(3),
    count: Some(5),
    scope: Some(NluScope::Restart),
    enabled: Some(true),
    mute: Some(false),
    repeat_mode: Some(NluRepeatMode::One),
    seconds: Some(-15),
    speed: Some(NluPlaybackSpeed::OnePointFive),
    direction: Some(NluDirection::Up),
    amount: Some(NluAmount::Large),
    level: Some(72),
    preset: Some("4".into()),
    view: Some(NluView::NowPlaying),
    phone_action: Some(NluPhoneAction::Answer),
    webapp_name: Some("weather".into()),
    uri: Some("spotify:track:1".into()),
    context_uri: Some("spotify:playlist:2".into()),
  }
}

#[test]
fn default_slots_carry_no_value() {
  let slots = NluSlots::default();
  assert_eq!(slots, NluSlots { ..Default::default() });
  assert!(slots.target.is_none());
  assert!(slots.target_type.is_none());
  assert!(slots.playlist.is_none());
  assert!(slots.genre.is_none());
  assert!(slots.mood.is_none());
  assert!(slots.era.is_none());
  assert!(slots.popularity_filter.is_none());
  assert!(slots.position.is_none());
  assert!(slots.count.is_none());
  assert!(slots.scope.is_none());
  assert!(slots.enabled.is_none());
  assert!(slots.mute.is_none());
  assert!(slots.repeat_mode.is_none());
  assert!(slots.seconds.is_none());
  assert!(slots.speed.is_none());
  assert!(slots.direction.is_none());
  assert!(slots.amount.is_none());
  assert!(slots.level.is_none());
  assert!(slots.preset.is_none());
  assert!(slots.view.is_none());
  assert!(slots.phone_action.is_none());
  assert!(slots.webapp_name.is_none());
  assert!(slots.uri.is_none());
  assert!(slots.context_uri.is_none());
}

#[test]
fn a_fast_path_hit_projects_into_a_resolved_intent_unchanged() {
  let hit = fast_path::match_transcript("play preset 3").expect("preset 3 matches");
  let resolved = NluResolvedIntent {
    intent: hit.intent.to_owned(),
    slots: hit.slots.clone(),
    transcript: "play preset 3".into(),
    alternates: None,
  };
  assert_eq!(resolved.intent, "PRESET_PLAY");
  assert_eq!(resolved.slots, hit.slots);
  assert_eq!(resolved.transcript, "play preset 3");
  assert_eq!(resolved.alternates, None);
}

#[test]
fn a_no_intent_resolution_keeps_the_transcript_and_drops_the_slots() {
  let resolved = NluResolvedIntent {
    intent: intent_catalog::NO_INTENT.to_owned(),
    slots: NluSlots::default(),
    transcript: "what is the airspeed velocity of an unladen swallow".into(),
    alternates: None,
  };
  assert_eq!(resolved.intent, "NO_INTENT");
  assert!(!intent_catalog::contains(&resolved.intent));
  assert_eq!(resolved.slots, NluSlots::default());
}

#[test]
fn absent_alternates_stay_distinct_from_an_empty_list() {
  let base = NluResolvedIntent {
    intent: "PLAY".into(),
    slots: NluSlots::default(),
    transcript: "play".into(),
    alternates: None,
  };
  let empty = NluResolvedIntent {
    alternates: Some(Vec::new()),
    ..base.clone()
  };
  assert_ne!(base, empty);

  let carried = NluResolvedIntent {
    alternates: Some(vec![NluAlternate {
      intent: "SEARCH".into(),
      slots: Some(every_slot_populated()),
    }]),
    ..base
  };
  let alternates = carried.alternates.expect("alternates present");
  assert_eq!(alternates.len(), 1);
  assert_eq!(alternates[0].slots.as_ref(), Some(&every_slot_populated()));
}

#[test]
fn every_slot_survives_a_wire_round_trip() {
  let resolved = NluResolvedIntent {
    intent: "SEARCH".into(),
    slots: every_slot_populated(),
    transcript: "spoken".into(),
    alternates: Some(vec![NluAlternate {
      intent: "PLAY".into(),
      slots: None,
    }]),
  };
  let encoded = serde_json::to_string(&resolved).expect("resolved intent serializes");
  let decoded: NluResolvedIntent = serde_json::from_str(&encoded).expect("resolved intent deserializes");
  assert_eq!(decoded, resolved);
}

#[test]
fn an_absent_slot_object_decodes_to_an_empty_slot_set() {
  let decoded: NluResolvedIntent =
    serde_json::from_str(r#"{"intent":"PLAY","transcript":"play"}"#).expect("slots default when absent");
  assert_eq!(decoded.slots, NluSlots::default());
  assert_eq!(decoded.alternates, None);
}
