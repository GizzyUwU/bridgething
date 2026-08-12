use std::sync::{
  Arc, Mutex,
  atomic::{AtomicUsize, Ordering},
};

use bridgething_companion::{
  backend::{NluModelOutputs, NluModelRunner, NluRunnerError},
  voice::{
    controller::{VoiceController, VoiceControllerConfig},
    inference::{BundleError, BundleInference, NluDecoding, NluInference},
    intent_catalog, slot_mapping,
  },
};
use libbridgething::{
  NluAmount, NluDirection, NluPhoneAction, NluPlaybackSpeed, NluPopularityFilter, NluRepeatMode, NluScope, NluSlots,
  NluStage, NluTargetType, NluView,
};
use nlu::{DecodedFrame, ManifestInfo, NluError, Rejection, SlotValue, TokenizedInput};

fn slots(pairs: &[(&str, &str)]) -> Vec<SlotValue> {
  pairs
    .iter()
    .map(|(name, value)| SlotValue {
      name: (*name).to_owned(),
      value: (*value).to_owned(),
    })
    .collect()
}

fn catalog() -> Vec<String> {
  intent_catalog::SURFACE_NAMES.iter().map(|n| (*n).to_owned()).collect()
}

// MARK: slot mapping

#[test]
fn span_slots_pass_through_verbatim() {
  let out = slot_mapping::apply(&slots(&[
    ("target", "héroes by beyoncé"),
    ("playlist", "workout"),
    ("genre", "jazz"),
    ("mood", "chill"),
    ("era", "80s"),
    ("webapp_name", "weather"),
    ("preset", "2"),
  ]));
  assert_eq!(out.target.as_deref(), Some("héroes by beyoncé"));
  assert_eq!(out.playlist.as_deref(), Some("workout"));
  assert_eq!(out.genre.as_deref(), Some("jazz"));
  assert_eq!(out.mood.as_deref(), Some("chill"));
  assert_eq!(out.era.as_deref(), Some("80s"));
  assert_eq!(out.webapp_name.as_deref(), Some("weather"));
  assert_eq!(out.preset.as_deref(), Some("2"));
}

#[test]
fn snake_case_yaml_tokens_resolve_to_the_wire_enums() {
  let out = slot_mapping::apply(&slots(&[
    ("target_type", "station"),
    ("popularity_filter", "top_5"),
    ("scope", "previous_track"),
    ("view", "now_playing"),
    ("repeat_mode", "one"),
    ("direction", "up"),
    ("amount", "large"),
    ("phone_action", "unhold"),
  ]));
  assert_eq!(out.target_type, Some(NluTargetType::Station));
  assert_eq!(out.popularity_filter, Some(NluPopularityFilter::Top5));
  assert_eq!(out.scope, Some(NluScope::PreviousTrack));
  assert_eq!(out.view, Some(NluView::NowPlaying));
  assert_eq!(out.repeat_mode, Some(NluRepeatMode::One));
  assert_eq!(out.direction, Some(NluDirection::Up));
  assert_eq!(out.amount, Some(NluAmount::Large));
  assert_eq!(out.phone_action, Some(NluPhoneAction::Unhold));
}

#[test]
fn playback_speed_is_matched_without_case_folding() {
  assert_eq!(
    slot_mapping::apply(&slots(&[("speed", "1.5")])).speed,
    Some(NluPlaybackSpeed::OnePointFive)
  );
  assert_eq!(
    slot_mapping::apply(&slots(&[("speed", "2")])).speed,
    Some(NluPlaybackSpeed::Two)
  );
}

#[test]
fn python_stringified_booleans_decode_either_case() {
  assert_eq!(slot_mapping::apply(&slots(&[("enabled", "True")])).enabled, Some(true));
  assert_eq!(
    slot_mapping::apply(&slots(&[("enabled", "false")])).enabled,
    Some(false)
  );
  assert_eq!(slot_mapping::apply(&slots(&[("mute", "true")])).mute, Some(true));
  assert_eq!(slot_mapping::apply(&slots(&[("mute", "False")])).mute, Some(false));
  assert_eq!(slot_mapping::apply(&slots(&[("enabled", "maybe")])).enabled, None);
}

#[test]
fn numeric_slots_parse_and_reject_non_numbers() {
  let out = slot_mapping::apply(&slots(&[
    ("count", "2"),
    ("position", "3"),
    ("level", "4"),
    ("seconds", "-30"),
  ]));
  assert_eq!(out.count, Some(2));
  assert_eq!(out.position, Some(3));
  assert_eq!(out.level, Some(4));
  assert_eq!(out.seconds, Some(-30));
  assert_eq!(slot_mapping::apply(&slots(&[("count", "a few")])).count, None);
  assert_eq!(slot_mapping::apply(&slots(&[("level", "-1")])).level, None);
}

#[test]
fn values_outside_a_wire_enum_are_dropped_rather_than_guessed() {
  assert_eq!(slot_mapping::apply(&slots(&[("view", "cover_flow")])).view, None);
  assert_eq!(
    slot_mapping::apply(&slots(&[("target_type", "audiobook")])).target_type,
    None
  );
  assert_eq!(
    slot_mapping::apply(&slots(&[("target_type", "hologram")])).target_type,
    None
  );
}

#[test]
fn slot_names_the_wire_shape_does_not_carry_are_ignored() {
  assert_eq!(
    slot_mapping::apply(&slots(&[("nonesuch", "value")])),
    NluSlots::default()
  );
}

#[test]
fn camel_folding_matches_the_generated_spellings() {
  assert_eq!(slot_mapping::camel("now_playing"), "nowPlaying");
  assert_eq!(slot_mapping::camel("top_5"), "top5");
  assert_eq!(slot_mapping::camel("previous_track"), "previousTrack");
  assert_eq!(slot_mapping::camel("album"), "album");
}

// MARK: bundle inference

#[derive(Default)]
struct DecoderCalls {
  tokenized: Option<String>,
  decoded_transcript: Option<String>,
  decoded_intent_logits: Option<Vec<f32>>,
  decoded_bio_logits: Option<Vec<f32>>,
  decoded_closed_logits: Option<Vec<Vec<f32>>>,
}

struct FakeDecoder {
  intent_names: Vec<String>,
  rejection: Option<Rejection>,
  frame: DecodedFrame,
  tokens: TokenizedInput,
  seen: Mutex<DecoderCalls>,
}

impl FakeDecoder {
  fn new() -> Self {
    Self {
      intent_names: catalog(),
      rejection: Some(Rejection {
        in_domain_threshold: 0.5,
        clarify_margin: 0.4,
      }),
      frame: DecodedFrame {
        intent: "PLAY".into(),
        slots: Vec::new(),
      },
      tokens: TokenizedInput {
        input_ids: (0..4).collect(),
        attention_mask: vec![1; 4],
        offset_starts: Vec::new(),
        offset_ends: Vec::new(),
      },
      seen: Mutex::new(DecoderCalls::default()),
    }
  }

  fn with_intents(mut self, names: Vec<String>) -> Self {
    self.intent_names = names;
    self
  }

  fn with_rejection(mut self, rejection: Option<Rejection>) -> Self {
    self.rejection = rejection;
    self
  }

  fn with_frame(mut self, frame: DecodedFrame) -> Self {
    self.frame = frame;
    self
  }
}

impl NluDecoding for FakeDecoder {
  fn info(&self) -> ManifestInfo {
    ManifestInfo {
      schema_version: "0.3.1".into(),
      max_len: self.tokens.input_ids.len() as u32,
      intent_names: self.intent_names.clone(),
      bio_tag_count: 13,
      closed_head_sizes: vec![2; 16],
      rejection: self.rejection,
    }
  }

  fn tokenize(&self, transcript: String) -> Result<TokenizedInput, NluError> {
    self.seen.lock().unwrap().tokenized = Some(transcript);
    Ok(self.tokens.clone())
  }

  fn decode(
    &self,
    transcript: String,
    _tokens: TokenizedInput,
    intent_logits: Vec<f32>,
    bio_logits: Vec<f32>,
    closed_logits: Vec<Vec<f32>>,
  ) -> Result<DecodedFrame, NluError> {
    let mut seen = self.seen.lock().unwrap();
    seen.decoded_transcript = Some(transcript);
    seen.decoded_intent_logits = Some(intent_logits);
    seen.decoded_bio_logits = Some(bio_logits);
    seen.decoded_closed_logits = Some(closed_logits);
    Ok(self.frame.clone())
  }
}

#[derive(Default)]
struct RunnerCalls {
  input_ids: Option<Vec<i32>>,
  attention_mask: Option<Vec<i32>>,
}

struct FakeRunner {
  outputs: NluModelOutputs,
  seen: Mutex<RunnerCalls>,
  warmed: AtomicUsize,
}

impl FakeRunner {
  fn new(outputs: NluModelOutputs) -> Arc<Self> {
    Arc::new(Self {
      outputs,
      seen: Mutex::new(RunnerCalls::default()),
      warmed: AtomicUsize::new(0),
    })
  }
}

impl NluModelRunner for FakeRunner {
  fn prewarm(&self) {
    self.warmed.fetch_add(1, Ordering::SeqCst);
  }

  fn predict(&self, input_ids: Vec<i32>, attention_mask: Vec<i32>) -> Result<NluModelOutputs, NluRunnerError> {
    let mut seen = self.seen.lock().unwrap();
    seen.input_ids = Some(input_ids);
    seen.attention_mask = Some(attention_mask);
    Ok(self.outputs.clone())
  }
}

fn outputs(ood: f32) -> NluModelOutputs {
  NluModelOutputs {
    intent_logits: vec![0.0; intent_catalog::SURFACE_NAMES.len()],
    ood_logit: ood,
    bio_logits: Vec::new(),
    closed_logits: Vec::new(),
  }
}

fn outputs_hot(intent: &str, ood: f32) -> NluModelOutputs {
  let mut out = outputs(ood);
  let index = intent_catalog::SURFACE_NAMES
    .iter()
    .position(|name| *name == intent)
    .expect("the intent is in the catalog");
  out.intent_logits[index] = 8.0;
  out
}

fn sigmoid(x: f64) -> f64 {
  1.0 / (1.0 + (-x).exp())
}

#[tokio::test]
async fn the_in_domain_logit_is_the_negated_ood_head() {
  let accepted = BundleInference::new(Arc::new(FakeDecoder::new()), FakeRunner::new(outputs(-8.0)))
    .unwrap()
    .infer("play some jazz")
    .await
    .unwrap();
  assert!((accepted.in_domain_logit - 8.0).abs() < 1e-6);
  assert!(
    sigmoid(accepted.in_domain_logit) >= 0.5,
    "a command must clear the in-domain threshold"
  );

  let rejected = BundleInference::new(Arc::new(FakeDecoder::new()), FakeRunner::new(outputs(8.0)))
    .unwrap()
    .infer("what is the capital of peru")
    .await
    .unwrap();
  assert!(
    sigmoid(rejected.in_domain_logit) < 0.5,
    "a non-command must fall below the in-domain threshold"
  );
}

#[tokio::test]
async fn tokens_and_every_head_reach_the_decoder_unchanged() {
  let decoder = Arc::new(FakeDecoder::new().with_frame(DecodedFrame {
    intent: "PLAY".into(),
    slots: slots(&[("target", "1989 by taylor swift"), ("target_type", "album")]),
  }));
  let mut model = outputs_hot("PLAY", -8.0);
  model.bio_logits = vec![0.25; 52];
  model.closed_logits = vec![vec![0.5, 0.5]; 16];
  let out = BundleInference::new(decoder.clone(), FakeRunner::new(model.clone()))
    .unwrap()
    .infer("play the album 1989 by taylor swift")
    .await
    .unwrap();

  let seen = decoder.seen.lock().unwrap();
  assert_eq!(seen.tokenized.as_deref(), Some("play the album 1989 by taylor swift"));
  assert_eq!(
    seen.decoded_transcript.as_deref(),
    Some("play the album 1989 by taylor swift")
  );
  assert_eq!(seen.decoded_intent_logits.as_ref(), Some(&model.intent_logits));
  assert_eq!(seen.decoded_bio_logits.as_ref(), Some(&model.bio_logits));
  assert_eq!(seen.decoded_closed_logits.as_ref(), Some(&model.closed_logits));

  let widened: Vec<f64> = model.intent_logits.iter().map(|logit| *logit as f64).collect();
  assert_eq!(out.intent_logits, widened);
  assert_eq!(out.slots.target.as_deref(), Some("1989 by taylor swift"));
  assert_eq!(out.slots.target_type, Some(NluTargetType::Album));
}

#[tokio::test]
async fn tokenizer_output_is_what_the_runner_sees() {
  let decoder = Arc::new(FakeDecoder::new());
  let runner = FakeRunner::new(outputs(-8.0));
  BundleInference::new(decoder.clone(), runner.clone())
    .unwrap()
    .infer("turn it up")
    .await
    .unwrap();

  let seen = runner.seen.lock().unwrap();
  assert_eq!(seen.input_ids.as_ref(), Some(&decoder.tokens.input_ids));
  assert_eq!(seen.attention_mask.as_ref(), Some(&decoder.tokens.attention_mask));
}

#[tokio::test]
async fn decoded_slots_land_on_the_wire_shape() {
  let decoder = Arc::new(FakeDecoder::new().with_frame(DecodedFrame {
    intent: "SET_VOLUME".into(),
    slots: slots(&[("direction", "up")]),
  }));
  let out = BundleInference::new(decoder, FakeRunner::new(outputs(-8.0)))
    .unwrap()
    .infer("turn it up")
    .await
    .unwrap();
  assert_eq!(out.slots.direction, Some(NluDirection::Up));
}

#[test]
fn a_bundle_whose_intents_are_not_the_catalog_is_refused() {
  let mut shortened = catalog();
  shortened.pop();
  let refusal = BundleInference::new(
    Arc::new(FakeDecoder::new().with_intents(shortened)),
    FakeRunner::new(outputs(-8.0)),
  )
  .err();
  assert!(matches!(refusal, Some(BundleError::CatalogMismatch { .. })));
}

#[test]
fn intent_order_is_part_of_the_contract_not_just_membership() {
  let mut reversed = catalog();
  reversed.reverse();
  assert!(
    BundleInference::new(
      Arc::new(FakeDecoder::new().with_intents(reversed)),
      FakeRunner::new(outputs(-8.0))
    )
    .is_err(),
    "a reordered head means every label index is wrong"
  );
}

#[test]
fn the_bundles_calibrated_operating_point_is_surfaced() {
  let decoder = Arc::new(FakeDecoder::new().with_rejection(Some(Rejection {
    in_domain_threshold: 0.62,
    clarify_margin: 0.4,
  })));
  let rejection = BundleInference::new(decoder, FakeRunner::new(outputs(-8.0)))
    .unwrap()
    .rejection()
    .expect("the bundle carries a sweep");
  assert!((rejection.in_domain_threshold - 0.62).abs() < 1e-9);
  assert!((rejection.clarify_margin - 0.4).abs() < 1e-9);
  assert_eq!(rejection.max_alternates, 2);
}

#[test]
fn an_export_without_a_sweep_surfaces_no_operating_point() {
  let decoder = Arc::new(FakeDecoder::new().with_rejection(None));
  assert!(
    BundleInference::new(decoder, FakeRunner::new(outputs(-8.0)))
      .unwrap()
      .rejection()
      .is_none()
  );
}

#[tokio::test]
async fn prewarm_reaches_the_runner() {
  let runner = FakeRunner::new(outputs(-8.0));
  BundleInference::new(Arc::new(FakeDecoder::new()), runner.clone())
    .unwrap()
    .prewarm()
    .await;
  assert_eq!(runner.warmed.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn the_bundles_rejection_flows_through_the_controller_end_to_end() {
  let inference = Arc::new(
    BundleInference::new(
      Arc::new(FakeDecoder::new().with_frame(DecodedFrame {
        intent: "SEARCH".into(),
        slots: slots(&[("genre", "shoegaze")]),
      })),
      FakeRunner::new(outputs_hot("SEARCH", -6.0)),
    )
    .unwrap(),
  );
  let config = VoiceControllerConfig {
    rejection: inference.rejection().unwrap_or_default(),
    ..VoiceControllerConfig::default()
  };
  let resolution = VoiceController::new(Some(inference), config)
    .resolve("find me some nineties shoegaze")
    .await
    .unwrap();
  assert_eq!(resolution.stage, NluStage::Model);
  assert_eq!(resolution.resolved.intent, "SEARCH");
  assert_eq!(resolution.resolved.slots.genre.as_deref(), Some("shoegaze"));
}
