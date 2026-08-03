use std::path::Path;

use tokenizers::Tokenizer;

use crate::error::{NluError, Result};

#[derive(Debug, Clone, uniffi::Record)]
pub struct TokenizedInput {
  pub input_ids: Vec<i32>,
  pub attention_mask: Vec<i32>,
  pub offset_starts: Vec<u32>,
  pub offset_ends: Vec<u32>,
}

pub struct TranscriptTokenizer {
  inner: Tokenizer,
  max_len: usize,
  pad_id: i32,
}

impl TranscriptTokenizer {
  pub fn load(path: &Path, max_len: usize) -> Result<Self> {
    let inner = Tokenizer::from_file(path).map_err(|e| NluError::BundleLoad {
      msg: format!("{}: {e}", path.display()),
    })?;
    let pad_id = ["[PAD]", "<pad>", "<|padding|>"]
      .iter()
      .find_map(|t| inner.token_to_id(t))
      .map(|id| id as i32)
      .unwrap_or(0);
    Ok(Self { inner, max_len, pad_id })
  }

  pub fn encode(&self, transcript: &str) -> Result<TokenizedInput> {
    let encoding = self
      .inner
      .encode_char_offsets(transcript, true)
      .map_err(|e| NluError::Tokenizer { msg: e.to_string() })?;

    let mut input_ids: Vec<i32> = encoding.get_ids().iter().map(|&id| id as i32).collect();
    let mut attention_mask: Vec<i32> = encoding.get_attention_mask().iter().map(|&m| m as i32).collect();
    let mut offsets: Vec<(u32, u32)> = encoding
      .get_offsets()
      .iter()
      .map(|&(start, end)| (start as u32, end as u32))
      .collect();

    input_ids.truncate(self.max_len);
    attention_mask.truncate(self.max_len);
    offsets.truncate(self.max_len);

    while input_ids.len() < self.max_len {
      input_ids.push(self.pad_id);
      attention_mask.push(0);
      offsets.push((0, 0));
    }

    Ok(TokenizedInput {
      input_ids,
      attention_mask,
      offset_starts: offsets.iter().map(|o| o.0).collect(),
      offset_ends: offsets.iter().map(|o| o.1).collect(),
    })
  }
}
