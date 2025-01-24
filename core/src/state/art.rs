use std::sync::Arc;

use quick_cache::{sync::Cache, Weighter};

#[derive(Clone)]
struct ImageWeighter;

impl Weighter<String, Vec<u8>> for ImageWeighter {
  fn weight(&self, _key: &String, val: &Vec<u8>) -> u64 {
    val.len() as u64
  }
}

pub type CoverArtCache = Arc<ImageCache>;

// this is in its own struct in case i want to add disk persistence later :)
#[derive(Debug)]
pub struct ImageCache {
  cache: Cache<String, Vec<u8>, ImageWeighter>,
}

impl ImageCache {
  pub fn new() -> Self {
    Self {
      cache: Cache::with_weighter(256, 8 * 1024 * 1024, ImageWeighter),
    }
  }

  pub fn get(&self, key: &String) -> Option<Vec<u8>> {
    self.cache.get(key)
  }

  pub fn insert(&self, key: String, value: Vec<u8>) {
    self.cache.insert(key, value)
  }
}

impl Default for ImageCache {
  fn default() -> Self {
    Self::new()
  }
}
