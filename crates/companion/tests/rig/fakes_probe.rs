use std::sync::{Arc, Mutex};

use bridgething_companion::provider::ProviderAuthState;

use crate::fakes::{FakeSource, SourceCatalog};

impl SourceCatalog {
  pub fn searched(&self) -> Vec<String> {
    self.searched.lock().unwrap().clone()
  }
}

impl FakeSource {
  pub fn resolving(name: &str, uri: &str) -> Arc<Self> {
    FakeSource::build(
      name,
      Some(Arc::new(SourceCatalog {
        uri: uri.to_owned(),
        searched: Mutex::new(Vec::new()),
      })),
    )
  }

  pub fn catalog(&self) -> Arc<SourceCatalog> {
    self.catalog.clone().expect("the source was built with a catalog")
  }

  pub fn connectivity_heard(&self) -> Vec<bool> {
    self.connectivity.lock().unwrap().clone()
  }

  pub fn report_auth(&self, state: ProviderAuthState) {
    let held = self.auth.lock().unwrap().clone();
    if let Some(observer) = held {
      observer(state);
    }
  }
}
