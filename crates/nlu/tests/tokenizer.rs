use nlu::tokenize::TranscriptTokenizer;

fn tokenizer() -> Option<TranscriptTokenizer> {
  let path = std::env::var("BRIDGETHING_NLU_TOKENIZER").ok()?;
  Some(TranscriptTokenizer::load(path.as_ref(), 64).expect("tokenizer loads"))
}

fn slice_chars(text: &str, start: u32, end: u32) -> String {
  text.chars().skip(start as usize).take((end - start) as usize).collect()
}

#[test]
fn offsets_are_char_positions_into_the_transcript() {
  let Some(tok) = tokenizer() else {
    eprintln!("BRIDGETHING_NLU_TOKENIZER unset; skipping");
    return;
  };
  let transcript = "play héroes by beyoncé now";
  let encoded = tok.encode(transcript).unwrap();

  let mut rebuilt = String::new();
  for i in 0..encoded.input_ids.len() {
    if encoded.attention_mask[i] == 0 || encoded.offset_ends[i] <= encoded.offset_starts[i] {
      continue;
    }
    rebuilt.push_str(&slice_chars(
      transcript,
      encoded.offset_starts[i],
      encoded.offset_ends[i],
    ));
  }
  let squashed: String = rebuilt.chars().filter(|c| !c.is_whitespace()).collect();
  let expected: String = transcript.chars().filter(|c| !c.is_whitespace()).collect();
  assert_eq!(squashed, expected, "token offsets must reassemble the transcript");
}

#[test]
fn encoding_is_fixed_length_with_a_padded_tail() {
  let Some(tok) = tokenizer() else {
    eprintln!("BRIDGETHING_NLU_TOKENIZER unset; skipping");
    return;
  };
  let encoded = tok.encode("pause").unwrap();
  assert_eq!(encoded.input_ids.len(), 64);
  assert_eq!(encoded.attention_mask.len(), 64);
  assert_eq!(encoded.offset_starts.len(), 64);
  assert_eq!(*encoded.attention_mask.last().unwrap(), 0);
  assert!(
    encoded.attention_mask.iter().sum::<i32>() >= 3,
    "cls + token + sep at minimum"
  );
}
