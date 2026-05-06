//! Stock preset persistence.
//!
//! Stock presets sit in the same per-app KV namespace `client.store`
//! writes to: `kv.data_*` under `Uuid::nil()`. A modern webapp running
//! without an active id (`active_webapp` returns `None`) reads/writes
//! the same scope through `Store.Get`/`Put`/`Delete`, so a debugging
//! webapp can inspect or edit stock presets via the same wire surface.
//!
//! Stock app sends one preset at a time on SetPreset and expects the
//! complete preset list back, so we walk all four slots on each list.
//! Slots are stored as their own JSON blobs (`presets:1`..`presets:4`)
//! to avoid contention on per-slot writes.

use libbridgething::stock::StockPreset;
use uuid::Uuid;

use crate::state::{KvStore, StateResult};

const PRESET_SLOTS: [usize; 4] = [1, 2, 3, 4];
const STOCK_SCOPE: Uuid = Uuid::nil();

fn key(slot: usize) -> String {
  format!("presets:{slot}")
}

pub async fn list(kv: &KvStore) -> StateResult<Vec<StockPreset>> {
  let mut out = Vec::with_capacity(PRESET_SLOTS.len());
  for slot in PRESET_SLOTS {
    if let Some(raw) = kv.data_get(STOCK_SCOPE, &key(slot)).await?
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
  kv.data_set(STOCK_SCOPE, &key(preset.slot_index), value).await
}
