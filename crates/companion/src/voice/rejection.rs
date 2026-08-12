use libbridgething::NluSlots;

use crate::voice::intent_catalog;

#[derive(Debug, Clone)]
pub struct InferenceOutput {
  pub intent_logits: Vec<f64>,
  pub in_domain_logit: f64,
  pub slots: NluSlots,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RejectionPolicy {
  pub in_domain_threshold: f64,
  pub clarify_margin: f64,
  pub max_alternates: usize,
}

impl Default for RejectionPolicy {
  fn default() -> Self {
    Self {
      in_domain_threshold: 0.5,
      clarify_margin: 0.15,
      max_alternates: 2,
    }
  }
}

#[derive(Debug, Clone, PartialEq)]
pub enum RejectionOutcome {
  Accept { intent: &'static str },
  NoIntent,
  Clarify { alternates: Vec<&'static str> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum RejectionError {
  #[error("intent head emits {logits} logits but the catalog has {catalog} names")]
  HeadMismatch { logits: usize, catalog: usize },
}

pub fn evaluate(output: &InferenceOutput, policy: RejectionPolicy) -> Result<RejectionOutcome, RejectionError> {
  let names = intent_catalog::SURFACE_NAMES;
  if output.intent_logits.len() != names.len() {
    return Err(RejectionError::HeadMismatch {
      logits: output.intent_logits.len(),
      catalog: names.len(),
    });
  }

  let in_domain_accepts = sigmoid(output.in_domain_logit) >= policy.in_domain_threshold;
  if !in_domain_accepts {
    return Ok(RejectionOutcome::NoIntent);
  }

  let mut ranked: Vec<(&'static str, f64)> = softmax(&output.intent_logits)
    .into_iter()
    .enumerate()
    .map(|(index, probability)| (names[index], probability))
    .collect();
  ranked.sort_by(|left, right| right.1.total_cmp(&left.1));

  let Some(&(top, top_probability)) = ranked.first() else {
    return Ok(RejectionOutcome::NoIntent);
  };
  let Some(&(_, runner_up_probability)) = ranked.get(1) else {
    return Ok(RejectionOutcome::Accept { intent: top });
  };

  if top_probability - runner_up_probability < policy.clarify_margin {
    return Ok(RejectionOutcome::Clarify {
      alternates: ranked
        .iter()
        .take(policy.max_alternates)
        .map(|(name, _)| *name)
        .collect(),
    });
  }
  Ok(RejectionOutcome::Accept { intent: top })
}

pub fn sigmoid(x: f64) -> f64 {
  1.0 / (1.0 + (-x).exp())
}

pub fn softmax(logits: &[f64]) -> Vec<f64> {
  let Some(peak) = logits.iter().copied().reduce(f64::max) else {
    return Vec::new();
  };
  let exponentials: Vec<f64> = logits.iter().map(|logit| (logit - peak).exp()).collect();
  let total: f64 = exponentials.iter().sum();
  if total.is_nan() || total <= 0.0 {
    return vec![0.0; logits.len()];
  }
  exponentials.into_iter().map(|value| value / total).collect()
}
