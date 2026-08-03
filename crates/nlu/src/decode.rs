use crate::{
  error::{NluError, Result},
  manifest::{CLOSED_NONE, Manifest},
  tokenize::TokenizedInput,
};

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct SlotValue {
  pub name: String,
  pub value: String,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct DecodedFrame {
  pub intent: String,
  pub slots: Vec<SlotValue>,
}

pub fn decode(
  manifest: &Manifest,
  transcript: &str,
  tokens: &TokenizedInput,
  intent_logits: &[f32],
  bio_logits: &[f32],
  closed_logits: &[Vec<f32>],
) -> Result<DecodedFrame> {
  let n_intents = manifest.intents.len();
  if intent_logits.len() != n_intents {
    return Err(NluError::ShapeMismatch {
      msg: format!(
        "intent head has {} logits, manifest has {n_intents}",
        intent_logits.len()
      ),
    });
  }
  let n_bio = manifest.bio_tags.len();
  let seq = tokens.input_ids.len();
  if bio_logits.len() != seq * n_bio {
    return Err(NluError::ShapeMismatch {
      msg: format!("bio head has {} logits, expected {seq}x{n_bio}", bio_logits.len()),
    });
  }
  if closed_logits.len() != manifest.closed_heads.len() {
    return Err(NluError::ShapeMismatch {
      msg: format!(
        "{} closed heads, manifest has {}",
        closed_logits.len(),
        manifest.closed_heads.len()
      ),
    });
  }

  let intent_idx = argmax(intent_logits);
  let intent = manifest
    .intent_name(intent_idx)
    .ok_or_else(|| NluError::ShapeMismatch {
      msg: format!("intent index {intent_idx} out of range"),
    })?
    .to_string();
  let declared = manifest.declared_slots(&intent).unwrap_or(&[]);

  let mut slots = decode_spans(manifest, transcript, tokens, bio_logits, n_bio);
  for (head, logits) in manifest.closed_heads.iter().zip(closed_logits) {
    if logits.len() != head.values.len() {
      return Err(NluError::ShapeMismatch {
        msg: format!(
          "closed head {:?} has {} logits, manifest has {} values",
          head.slot,
          logits.len(),
          head.values.len()
        ),
      });
    }
    if !declared.iter().any(|d| d == &head.slot) {
      continue;
    }
    let value = &head.values[argmax(logits)];
    if value != CLOSED_NONE {
      slots.push(SlotValue {
        name: head.slot.clone(),
        value: value.clone(),
      });
    }
  }

  Ok(DecodedFrame { intent, slots })
}

fn decode_spans(
  manifest: &Manifest,
  transcript: &str,
  tokens: &TokenizedInput,
  bio_logits: &[f32],
  n_bio: usize,
) -> Vec<SlotValue> {
  let mut spans: Vec<SlotValue> = Vec::new();
  let mut current: Option<(String, u32, u32)> = None;

  let flush = |current: &mut Option<(String, u32, u32)>, spans: &mut Vec<SlotValue>| {
    if let Some((slot, start, end)) = current.take()
      && end > start
      && !spans.iter().any(|s| s.name == slot)
    {
      let value = snap(transcript, start as usize, end as usize);
      if !value.is_empty() {
        spans.push(SlotValue { name: slot, value });
      }
    }
  };

  for pos in 0..tokens.input_ids.len() {
    if tokens.offset_ends[pos] <= tokens.offset_starts[pos] {
      continue;
    }
    let row = &bio_logits[pos * n_bio..(pos + 1) * n_bio];
    let tag = &manifest.bio_tags[argmax(row)];
    let (start, end) = (tokens.offset_starts[pos], tokens.offset_ends[pos]);

    if tag == "O" {
      flush(&mut current, &mut spans);
      continue;
    }
    let (prefix, slot) = tag.split_at(2);
    match (&mut current, prefix) {
      (Some((cur, _, cur_end)), "I-") if cur == slot => *cur_end = end,
      _ => {
        flush(&mut current, &mut spans);
        current = Some((slot.to_string(), start, end));
      }
    }
  }
  flush(&mut current, &mut spans);
  spans
}

fn snap(transcript: &str, start: usize, end: usize) -> String {
  let chars: Vec<char> = transcript.chars().collect();
  let mut start = start.min(chars.len());
  let mut end = end.min(chars.len());
  while start > 0 && start < chars.len() && chars[start - 1].is_alphanumeric() && chars[start].is_alphanumeric() {
    start -= 1;
  }
  while end < chars.len() && end > 0 && chars[end].is_alphanumeric() && chars[end - 1].is_alphanumeric() {
    end += 1;
  }
  chars[start..end].iter().collect::<String>().trim().to_string()
}

fn argmax(values: &[f32]) -> usize {
  values
    .iter()
    .enumerate()
    .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
    .map(|(i, _)| i)
    .unwrap_or(0)
}

#[cfg(test)]
mod tests {
  use super::*;

  fn manifest() -> Manifest {
    serde_json::from_str(
      r#"{
              "schemaVersion": "0.3.1",
              "maxLen": 10,
              "intents": [
                {"name": "NEXT", "slots": ["count"]},
                {"name": "PLAY", "slots": ["target", "target_type", "genre"]}
              ],
              "bioTags": ["O", "B-genre", "I-genre", "B-target", "I-target"],
              "closedHeads": [
                {"slot": "count", "values": ["<none>", "2", "3"]},
                {"slot": "target_type", "values": ["<none>", "album", "track"]}
              ]
            }"#,
    )
    .unwrap()
  }

  const TRANSCRIPT: &str = "play blue rev by alvvays";

  fn tokens() -> TokenizedInput {
    let offsets: [(u32, u32); 10] = [
      (0, 0),   // cls
      (0, 4),   // play
      (4, 9),   // " blue"
      (9, 13),  // " rev"
      (13, 16), // " by"
      (16, 20), // " alv" (subword cut)
      (20, 24), // "vays"
      (0, 0),   // sep
      (0, 0),   // pad
      (0, 0),   // pad
    ];
    TokenizedInput {
      input_ids: vec![1; 10],
      attention_mask: vec![1, 1, 1, 1, 1, 1, 1, 1, 0, 0],
      offset_starts: offsets.iter().map(|o| o.0).collect(),
      offset_ends: offsets.iter().map(|o| o.1).collect(),
    }
  }

  fn one_hot(len: usize, hot: usize) -> Vec<f32> {
    (0..len).map(|i| if i == hot { 5.0 } else { 0.0 }).collect()
  }

  fn bio_rows(tags: [usize; 10]) -> Vec<f32> {
    tags.iter().flat_map(|&t| one_hot(5, t)).collect()
  }

  #[test]
  fn spans_join_snap_and_filter_to_the_winning_intent() {
    let bio = bio_rows([0, 0, 3, 4, 0, 3, 0, 0, 0, 0]);
    let frame = decode(
      &manifest(),
      TRANSCRIPT,
      &tokens(),
      &one_hot(2, 1),
      &bio,
      &[one_hot(3, 1), one_hot(3, 1)],
    )
    .unwrap();
    assert_eq!(frame.intent, "PLAY");
    assert_eq!(
      frame.slots,
      vec![
        SlotValue {
          name: "target".into(),
          value: "blue rev".into()
        },
        SlotValue {
          name: "target_type".into(),
          value: "album".into()
        },
      ]
    );
  }

  #[test]
  fn subword_spans_snap_to_whole_words() {
    let bio = bio_rows([0, 0, 0, 0, 0, 3, 0, 0, 0, 0]);
    let frame = decode(
      &manifest(),
      TRANSCRIPT,
      &tokens(),
      &one_hot(2, 1),
      &bio,
      &[one_hot(3, 0), one_hot(3, 0)],
    )
    .unwrap();
    assert_eq!(
      frame.slots,
      vec![SlotValue {
        name: "target".into(),
        value: "alvvays".into()
      }]
    );
  }

  #[test]
  fn an_i_tag_after_o_starts_its_own_span() {
    let bio = bio_rows([0, 0, 0, 4, 0, 0, 0, 0, 0, 0]);
    let frame = decode(
      &manifest(),
      TRANSCRIPT,
      &tokens(),
      &one_hot(2, 1),
      &bio,
      &[one_hot(3, 0), one_hot(3, 0)],
    )
    .unwrap();
    assert_eq!(
      frame.slots,
      vec![SlotValue {
        name: "target".into(),
        value: "rev".into()
      }]
    );
  }

  #[test]
  fn closed_slots_filter_to_the_declaring_intent_but_spans_ride_unfiltered() {
    let bio = bio_rows([0, 0, 3, 0, 0, 0, 0, 0, 0, 0]);
    let frame = decode(
      &manifest(),
      TRANSCRIPT,
      &tokens(),
      &one_hot(2, 0),
      &bio,
      &[one_hot(3, 1), one_hot(3, 2)],
    )
    .unwrap();
    assert_eq!(frame.intent, "NEXT");
    assert_eq!(
      frame.slots,
      vec![
        SlotValue {
          name: "target".into(),
          value: "blue".into()
        },
        SlotValue {
          name: "count".into(),
          value: "2".into()
        },
      ]
    );
  }

  #[test]
  fn shape_mismatches_are_typed_errors() {
    let bio = bio_rows([0; 10]);
    let short_intents = one_hot(1, 0);
    assert!(matches!(
      decode(
        &manifest(),
        TRANSCRIPT,
        &tokens(),
        &short_intents,
        &bio,
        &[one_hot(3, 0), one_hot(3, 0)]
      ),
      Err(NluError::ShapeMismatch { .. })
    ));
    assert!(matches!(
      decode(
        &manifest(),
        TRANSCRIPT,
        &tokens(),
        &one_hot(2, 0),
        &bio,
        &[one_hot(3, 0)]
      ),
      Err(NluError::ShapeMismatch { .. })
    ));
  }
}
