use bridgething_companion::voice::{
  intent_catalog,
  rejection::{self, InferenceOutput, RejectionError, RejectionOutcome, RejectionPolicy},
};

fn output_sized(logits: &[(&str, f64)], in_domain: f64, count: usize) -> InferenceOutput {
  let mut intent_logits = vec![0.0; count];
  for (name, logit) in logits {
    if let Some(index) = intent_catalog::SURFACE_NAMES.iter().position(|n| n == name)
      && index < count
    {
      intent_logits[index] = *logit;
    }
  }
  InferenceOutput {
    intent_logits,
    in_domain_logit: in_domain,
    slots: Default::default(),
  }
}

fn output(logits: &[(&str, f64)]) -> InferenceOutput {
  output_sized(logits, 8.0, intent_catalog::SURFACE_NAMES.len())
}

fn out_of_domain(logits: &[(&str, f64)]) -> InferenceOutput {
  output_sized(logits, -6.0, intent_catalog::SURFACE_NAMES.len())
}

fn clarify_alternates(outcome: RejectionOutcome) -> Vec<&'static str> {
  match outcome {
    RejectionOutcome::Clarify { alternates } => alternates,
    other => panic!("expected clarify, got {other:?}"),
  }
}

#[test]
fn a_clear_winner_in_domain_is_accepted() {
  let outcome = rejection::evaluate(&output(&[("PAUSE", 9.0)]), RejectionPolicy::default()).unwrap();
  assert_eq!(outcome, RejectionOutcome::Accept { intent: "PAUSE" });
}

#[test]
fn in_domain_head_below_threshold_yields_no_intent() {
  let outcome = rejection::evaluate(&out_of_domain(&[("PAUSE", 9.0)]), RejectionPolicy::default()).unwrap();
  assert_eq!(outcome, RejectionOutcome::NoIntent);
}

#[test]
fn out_of_domain_outranks_an_ambiguous_distribution() {
  let outcome = rejection::evaluate(
    &out_of_domain(&[("PLAY", 4.0), ("SEARCH", 4.0)]),
    RejectionPolicy::default(),
  )
  .unwrap();
  assert_eq!(outcome, RejectionOutcome::NoIntent);
}

#[test]
fn a_narrow_top_two_margin_yields_clarify_carrying_the_candidates() {
  let outcome = rejection::evaluate(&output(&[("PLAY", 4.0), ("SEARCH", 3.95)]), RejectionPolicy::default()).unwrap();
  let mut alternates = clarify_alternates(outcome);
  alternates.sort_unstable();
  assert_eq!(alternates, vec!["PLAY", "SEARCH"]);
}

#[test]
fn max_alternates_caps_the_candidate_list() {
  let policy = RejectionPolicy {
    clarify_margin: 0.5,
    max_alternates: 3,
    ..RejectionPolicy::default()
  };
  let outcome = rejection::evaluate(
    &output(&[("PLAY", 4.0), ("SEARCH", 4.0), ("NEXT", 4.0), ("PAUSE", 4.0)]),
    policy,
  )
  .unwrap();
  assert_eq!(clarify_alternates(outcome).len(), 3);
}

#[test]
fn a_widened_margin_turns_an_accepted_intent_into_clarify() {
  let logits = output(&[("PLAY", 4.0), ("SEARCH", 3.0)]);
  let narrow = RejectionPolicy {
    clarify_margin: 0.01,
    ..RejectionPolicy::default()
  };
  assert_eq!(
    rejection::evaluate(&logits, narrow).unwrap(),
    RejectionOutcome::Accept { intent: "PLAY" }
  );
  let wide = RejectionPolicy {
    clarify_margin: 0.9,
    ..RejectionPolicy::default()
  };
  let outcome = rejection::evaluate(&logits, wide).unwrap();
  assert!(
    matches!(outcome, RejectionOutcome::Clarify { .. }),
    "a 0.9 margin should not accept a 1.0-logit gap, got {outcome:?}"
  );
}

#[test]
fn a_head_that_disagrees_with_the_catalog_is_an_error_rather_than_a_guess() {
  let short = output_sized(&[("PAUSE", 9.0)], 8.0, 12);
  assert_eq!(
    rejection::evaluate(&short, RejectionPolicy::default()),
    Err(RejectionError::HeadMismatch {
      logits: 12,
      catalog: intent_catalog::SURFACE_NAMES.len(),
    })
  );

  let long = output_sized(&[("PAUSE", 9.0)], 8.0, intent_catalog::SURFACE_NAMES.len() + 1);
  assert!(rejection::evaluate(&long, RejectionPolicy::default()).is_err());
}

#[test]
fn softmax_is_stable_on_large_logits_and_sums_to_one() {
  let probabilities = rejection::softmax(&[900.0, 901.0, 899.0]);
  assert!(
    probabilities.iter().all(|p| p.is_finite()),
    "softmax overflowed: {probabilities:?}"
  );
  assert!((probabilities.iter().sum::<f64>() - 1.0).abs() < 1e-9);
}

#[test]
fn softmax_of_nothing_is_nothing() {
  assert!(rejection::softmax(&[]).is_empty());
}

#[test]
fn softmax_never_emits_a_nan() {
  let probabilities = rejection::softmax(&[f64::NEG_INFINITY, f64::NEG_INFINITY]);
  assert_eq!(probabilities, vec![0.0, 0.0]);
}

#[test]
fn the_ood_head_is_negated_into_the_in_domain_logit() {
  for x in [-6.0, -3.0, -0.5, 0.0, 0.5, 3.0, 6.0] {
    assert!(
      (rejection::sigmoid(-x) - (1.0 - rejection::sigmoid(x))).abs() < 1e-12,
      "sigmoid(-x) must equal 1 - sigmoid(x) at {x}"
    );
  }

  let ood = 3.0_f64;
  let accepting = RejectionPolicy {
    in_domain_threshold: 1.0 - rejection::sigmoid(ood) - 1e-6,
    ..RejectionPolicy::default()
  };
  let rejecting = RejectionPolicy {
    in_domain_threshold: 1.0 - rejection::sigmoid(ood) + 1e-6,
    ..RejectionPolicy::default()
  };
  let out = output_sized(&[("PAUSE", 9.0)], -ood, intent_catalog::SURFACE_NAMES.len());
  assert_eq!(
    rejection::evaluate(&out, accepting).unwrap(),
    RejectionOutcome::Accept { intent: "PAUSE" }
  );
  assert_eq!(
    rejection::evaluate(&out, rejecting).unwrap(),
    RejectionOutcome::NoIntent
  );
}

#[test]
fn a_nan_in_domain_head_yields_no_intent_rather_than_a_wrong_fire() {
  let broken = output_sized(&[("PAUSE", 9.0)], f64::NAN, intent_catalog::SURFACE_NAMES.len());
  assert_eq!(
    rejection::evaluate(&broken, RejectionPolicy::default()).unwrap(),
    RejectionOutcome::NoIntent
  );

  let unusable = RejectionPolicy {
    in_domain_threshold: f64::NAN,
    ..RejectionPolicy::default()
  };
  assert_eq!(
    rejection::evaluate(&output(&[("PAUSE", 9.0)]), unusable).unwrap(),
    RejectionOutcome::NoIntent
  );
}

#[test]
fn the_default_threshold_sits_exactly_at_a_zero_logit() {
  let policy = RejectionPolicy::default();
  assert_eq!(policy.in_domain_threshold, 0.5);
  assert_eq!(policy.clarify_margin, 0.15);
  assert_eq!(policy.max_alternates, 2);

  let at_zero = output_sized(&[("PAUSE", 9.0)], 0.0, intent_catalog::SURFACE_NAMES.len());
  assert_eq!(
    rejection::evaluate(&at_zero, policy).unwrap(),
    RejectionOutcome::Accept { intent: "PAUSE" }
  );
}

#[test]
fn tied_probabilities_rank_in_catalog_order() {
  let policy = RejectionPolicy {
    clarify_margin: 1.0,
    max_alternates: 4,
    ..RejectionPolicy::default()
  };
  let outcome = rejection::evaluate(
    &output(&[("SEARCH", 4.0), ("PLAY", 4.0), ("PAUSE", 4.0), ("NEXT", 4.0)]),
    policy,
  )
  .unwrap();
  assert_eq!(clarify_alternates(outcome), vec!["NEXT", "PAUSE", "PLAY", "SEARCH"]);
}

#[test]
fn a_zero_cap_clarifies_with_no_candidates() {
  let policy = RejectionPolicy {
    clarify_margin: 1.0,
    max_alternates: 0,
    ..RejectionPolicy::default()
  };
  let outcome = rejection::evaluate(&output(&[("PLAY", 4.0), ("SEARCH", 4.0)]), policy).unwrap();
  assert_eq!(clarify_alternates(outcome), Vec::<&str>::new());
}

#[test]
fn a_single_class_head_never_reaches_the_ranking() {
  let single = InferenceOutput {
    intent_logits: vec![1.0],
    in_domain_logit: 8.0,
    slots: Default::default(),
  };
  assert!(rejection::evaluate(&single, RejectionPolicy::default()).is_err());
}
