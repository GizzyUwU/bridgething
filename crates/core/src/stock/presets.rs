//! Stock preset persistence via KvStore.
//!
//! Stock app sends one preset at a time on SetPreset and expects the
//! complete preset list back. We store each slot as its own JSON blob
//! under `stock:presets:{slot}` (slots 1-4) so per-slot writes don't
//! contend; GetPresets walks all four.
//!
//! The daemon has no metadata source for arbitrary URIs — image_url /
//! name / description are stored if the caller pre-populated them
//! (companion-mediated flows could) and otherwise default to `None`.
//! The stock webapp's UI degrades gracefully on missing metadata.

use libbridgething::stock::StockPreset;

use crate::state::{KvStore, StateResult};

const PRESET_SLOTS: [usize; 4] = [1, 2, 3, 4];

fn key(slot: usize) -> String {
  format!("stock:presets:{slot}")
}

pub async fn list(kv: &KvStore) -> StateResult<Vec<StockPreset>> {
  let mut out = Vec::with_capacity(PRESET_SLOTS.len());
  for slot in PRESET_SLOTS {
    if let Some(raw) = kv.get(&key(slot)).await?
      && let Ok(preset) = serde_json::from_str::<StockPreset>(&raw)
    {
      out.push(preset);
    }
  }
  Ok(out)
}

pub async fn upsert(kv: &KvStore, preset: &StockPreset) -> StateResult<()> {
  if !PRESET_SLOTS.contains(&preset.slot_index) {
    tracing::warn!(slot = preset.slot_index, "stock preset slot out of range; ignoring");
    return Ok(());
  }
  let value = serde_json::to_string(preset).expect("StockPreset is always serializable");
  kv.set(key(preset.slot_index), value).await
}
