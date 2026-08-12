use std::{
  collections::BTreeMap,
  fs,
  path::{Path, PathBuf},
  sync::Mutex,
};

use serde::{Deserialize, Serialize};

use crate::seam::{CachedResource, SlotIndex};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Entry {
  digest: String,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  mime: Option<String>,
}

pub struct FsSlotIndex {
  path: PathBuf,
  held: Mutex<BTreeMap<String, Entry>>,
}

impl FsSlotIndex {
  pub fn new(path: impl Into<PathBuf>) -> Self {
    let path = path.into();
    let held = read(&path);
    Self {
      path,
      held: Mutex::new(held),
    }
  }

  fn persist(&self, held: &BTreeMap<String, Entry>) -> Result<(), String> {
    if let Some(parent) = self.path.parent() {
      fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let body = serde_json::to_vec(held).map_err(|err| err.to_string())?;
    let staged = self.path.with_extension(format!("{}.part", uuid::Uuid::now_v7()));
    fs::write(&staged, &body).map_err(|err| err.to_string())?;
    fs::rename(&staged, &self.path)
      .inspect_err(|_| {
        let _ = fs::remove_file(&staged);
      })
      .map_err(|err| err.to_string())
  }
}

impl SlotIndex for FsSlotIndex {
  fn get(&self, slot: &str) -> Option<CachedResource> {
    self.held.lock().unwrap().get(slot).map(resource)
  }

  fn set(&self, slot: &str, resource: &CachedResource) -> Result<(), String> {
    let mut held = self.held.lock().unwrap();
    held.insert(
      slot.to_owned(),
      Entry {
        digest: resource.digest.clone(),
        mime: resource.mime.clone(),
      },
    );
    self.persist(&held)
  }

  fn remove(&self, slot: &str) -> Result<(), String> {
    let mut held = self.held.lock().unwrap();
    if held.remove(slot).is_none() {
      return Ok(());
    }
    self.persist(&held)
  }

  fn entries(&self) -> Vec<(String, CachedResource)> {
    self
      .held
      .lock()
      .unwrap()
      .iter()
      .map(|(slot, entry)| (slot.clone(), resource(entry)))
      .collect()
  }
}

fn resource(entry: &Entry) -> CachedResource {
  CachedResource {
    digest: entry.digest.clone(),
    mime: entry.mime.clone(),
  }
}

fn read(path: &Path) -> BTreeMap<String, Entry> {
  let Ok(body) = fs::read(path) else {
    return BTreeMap::new();
  };
  match serde_json::from_slice(&body) {
    Ok(held) => held,
    Err(err) => {
      tracing::warn!(path = %path.display(), %err, "the slot index did not parse; every slot reads as empty");
      BTreeMap::new()
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn resource_of(digest: &str, mime: &str) -> CachedResource {
    CachedResource {
      digest: digest.to_owned(),
      mime: Some(mime.to_owned()),
    }
  }

  #[test]
  fn a_slot_survives_the_index_being_reopened() {
    let root = tempfile::tempdir().expect("a scratch directory");
    let path = root.path().join("slots.json");
    let held = resource_of(&"a".repeat(64), "image/png");

    FsSlotIndex::new(&path)
      .set("one__icon", &held)
      .expect("the slot writes");

    let reopened = FsSlotIndex::new(&path);
    assert_eq!(reopened.get("one__icon"), Some(held.clone()));
    assert_eq!(reopened.entries(), vec![("one__icon".to_string(), held)]);
  }

  #[test]
  fn removing_a_slot_that_is_not_there_is_a_no_op() {
    let root = tempfile::tempdir().expect("a scratch directory");
    let index = FsSlotIndex::new(root.path().join("slots.json"));

    index.remove("nothing__icon").expect("removing nothing is fine");
    assert!(index.entries().is_empty());
  }

  #[test]
  fn a_corrupt_index_reads_as_empty_rather_than_failing() {
    let root = tempfile::tempdir().expect("a scratch directory");
    let path = root.path().join("slots.json");
    fs::write(&path, b"{ this is not json").expect("the corruption lands");

    let index = FsSlotIndex::new(&path);
    assert!(index.entries().is_empty());
    assert_eq!(index.get("one__icon"), None);

    let held = resource_of(&"b".repeat(64), "text/html");
    index.set("one__settings", &held).expect("the slot writes");
    assert_eq!(FsSlotIndex::new(&path).get("one__settings"), Some(held));
  }

  #[test]
  fn a_slot_with_no_mime_round_trips_as_none() {
    let root = tempfile::tempdir().expect("a scratch directory");
    let path = root.path().join("slots.json");
    let held = CachedResource {
      digest: "c".repeat(64),
      mime: None,
    };

    FsSlotIndex::new(&path).set("one__overlay", &held).expect("it writes");
    assert_eq!(FsSlotIndex::new(&path).get("one__overlay"), Some(held));
  }
}
