use std::{collections::BTreeSet, path::Path};

use crate::store::{JsonFile, stored};

pub struct Sources(JsonFile<BTreeSet<String>>);

impl Sources {
  pub fn open(config_dir: &Path) -> Self {
    let path = config_dir.join("catalog-sources.json");
    let held = stored(&path).unwrap_or_default();
    Self(JsonFile::new(path, "catalog source list", held))
  }

  pub fn list(&self) -> Vec<String> {
    self.0.read(|held| held.iter().cloned().collect())
  }

  pub fn add(&self, url: String) -> Vec<String> {
    self.0.write(|held| {
      held.insert(url);
      held.iter().cloned().collect()
    })
  }

  pub fn remove(&self, url: &str) -> Vec<String> {
    self.0.write(|held| {
      held.remove(url);
      held.iter().cloned().collect()
    })
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn a_subscription_outlives_the_process_that_made_it() {
    let dir = tempfile::tempdir().expect("a scratch directory");

    let first = Sources::open(dir.path());
    assert_eq!(first.list(), Vec::<String>::new(), "a fresh host subscribes to nothing");
    first.add("https://apps.example/catalog.json".to_owned());
    first.add("https://other.example/catalog.json".to_owned());

    let reopened = Sources::open(dir.path());
    assert_eq!(
      reopened.list(),
      vec![
        "https://apps.example/catalog.json".to_owned(),
        "https://other.example/catalog.json".to_owned()
      ],
      "the list is read back from disk, not from a live handle"
    );

    let left = reopened.remove("https://other.example/catalog.json");
    assert_eq!(left, vec!["https://apps.example/catalog.json".to_owned()]);
    assert_eq!(
      Sources::open(dir.path()).list(),
      left,
      "a removal is flushed as eagerly as an addition"
    );
  }
}
