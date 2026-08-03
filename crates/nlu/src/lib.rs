pub mod decode;
pub mod error;
pub mod manifest;
pub mod tokenize;

use std::{path::Path, sync::Arc};

pub use decode::{DecodedFrame, SlotValue};
pub use error::{NluError, Result};
pub use manifest::{Manifest, Rejection};
pub use tokenize::TokenizedInput;

uniffi::setup_scaffolding!();

#[derive(uniffi::Object)]
pub struct NluDecoder {
  manifest: Manifest,
  tokenizer: tokenize::TranscriptTokenizer,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct ManifestInfo {
  pub schema_version: String,
  pub max_len: u32,
  pub intent_names: Vec<String>,
  pub bio_tag_count: u32,
  pub closed_head_sizes: Vec<u32>,
  pub rejection: Option<Rejection>,
}

#[uniffi::export]
impl NluDecoder {
  #[uniffi::constructor]
  pub fn load(bundle_dir: String) -> Result<Arc<Self>> {
    let dir = Path::new(&bundle_dir);
    let manifest = Manifest::load(&dir.join("manifest.json"))?;
    let tokenizer = tokenize::TranscriptTokenizer::load(&dir.join("tokenizer.json"), manifest.max_len as usize)?;
    Ok(Arc::new(Self { manifest, tokenizer }))
  }

  pub fn info(&self) -> ManifestInfo {
    ManifestInfo {
      schema_version: self.manifest.schema_version.clone(),
      max_len: self.manifest.max_len,
      intent_names: self.manifest.intents.iter().map(|i| i.name.clone()).collect(),
      bio_tag_count: self.manifest.bio_tags.len() as u32,
      closed_head_sizes: self
        .manifest
        .closed_heads
        .iter()
        .map(|h| h.values.len() as u32)
        .collect(),
      rejection: self.manifest.rejection,
    }
  }

  pub fn tokenize(&self, transcript: String) -> Result<TokenizedInput> {
    self.tokenizer.encode(&transcript)
  }

  pub fn decode(
    &self,
    transcript: String,
    tokens: TokenizedInput,
    intent_logits: Vec<f32>,
    bio_logits: Vec<f32>,
    closed_logits: Vec<Vec<f32>>,
  ) -> Result<DecodedFrame> {
    decode::decode(
      &self.manifest,
      &transcript,
      &tokens,
      &intent_logits,
      &bio_logits,
      &closed_logits,
    )
  }
}
