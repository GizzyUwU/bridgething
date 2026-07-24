use libbridgething::stock::StockPreset;
use uuid::Uuid;

use crate::state::{KvStore, StateResult};

const PRESET_SLOTS: [usize; 4] = [1, 2, 3, 4];
const PRESETS_KEY_PREFIX: &str = "presets:";
const STOCK_SCOPE: Uuid = Uuid::nil();

fn slot_key(slot: usize) -> String {
  format!("{PRESETS_KEY_PREFIX}{slot}")
}

pub async fn list(kv: &KvStore) -> StateResult<Vec<StockPreset>> {
  let rows = kv.data_list_prefix(STOCK_SCOPE, PRESETS_KEY_PREFIX).await?;
  let mut out: Vec<StockPreset> = rows
    .into_iter()
    .filter_map(|(_, raw)| serde_json::from_str::<StockPreset>(&raw).ok())
    .collect();
  out.sort_by_key(|p| p.slot_index);
  Ok(out)
}

pub async fn upsert_many(kv: &KvStore, presets: &[StockPreset]) -> StateResult<()> {
  let items: Vec<(String, String)> = presets
    .iter()
    .filter_map(|preset| {
      if !PRESET_SLOTS.contains(&preset.slot_index) {
        tracing::warn!(slot = preset.slot_index, "stock preset slot out of range; ignoring");
        return None;
      }
      let value = serde_json::to_string(preset).expect("StockPreset is always serializable");
      Some((slot_key(preset.slot_index), value))
    })
    .collect();
  if items.is_empty() {
    return Ok(());
  }
  kv.data_set_many(STOCK_SCOPE, items).await
}
