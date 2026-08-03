use std::path::Path;

use serde::Deserialize;

use crate::error::{NluError, Result};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Manifest {
  pub schema_version: String,
  pub max_len: u32,
  pub intents: Vec<IntentSpec>,
  pub bio_tags: Vec<String>,
  pub closed_heads: Vec<ClosedHead>,
  #[serde(default)]
  pub rejection: Option<Rejection>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IntentSpec {
  pub name: String,
  pub slots: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClosedHead {
  pub slot: String,
  pub values: Vec<String>,
}

#[derive(Debug, Clone, Copy, Deserialize, uniffi::Record)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Rejection {
  pub in_domain_threshold: f64,
  pub clarify_margin: f64,
}

pub const CLOSED_NONE: &str = "<none>";

impl Manifest {
  pub fn load(path: &Path) -> Result<Self> {
    let text = std::fs::read_to_string(path).map_err(|e| NluError::BundleLoad {
      msg: format!("{}: {e}", path.display()),
    })?;
    let manifest: Manifest =
      serde_json::from_str(&text).map_err(|e| NluError::ManifestInvalid { msg: e.to_string() })?;
    manifest.validate()?;
    Ok(manifest)
  }

  fn validate(&self) -> Result<()> {
    let invalid = |msg: String| Err(NluError::ManifestInvalid { msg });
    if self.intents.is_empty() {
      return invalid("no intents".into());
    }
    if !self.intents.windows(2).all(|w| w[0].name < w[1].name) {
      return invalid("intent order must be strictly alphabetical to match the exported head".into());
    }
    if self.bio_tags.first().map(String::as_str) != Some("O") {
      return invalid("bio tags must start with O".into());
    }
    for tag in self.bio_tags.iter().skip(1) {
      if tag.strip_prefix("B-").or_else(|| tag.strip_prefix("I-")).is_none() {
        return invalid(format!("bio tag {tag:?} is neither O nor B-/I- prefixed"));
      }
    }
    for head in &self.closed_heads {
      if head.values.first().map(String::as_str) != Some(CLOSED_NONE) {
        return invalid(format!("closed head {:?} must lead with {CLOSED_NONE}", head.slot));
      }
    }
    Ok(())
  }

  pub fn intent_name(&self, index: usize) -> Option<&str> {
    self.intents.get(index).map(|i| i.name.as_str())
  }

  pub fn declared_slots(&self, intent: &str) -> Option<&[String]> {
    self
      .intents
      .iter()
      .find(|i| i.name == intent)
      .map(|i| i.slots.as_slice())
  }
}
