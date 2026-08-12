use std::{
  fs,
  path::{Path, PathBuf},
  sync::Mutex,
};

const DEFAULT_ROUTE: &str = "/devices";

pub struct Route {
  path: PathBuf,
  held: Mutex<String>,
}

impl Route {
  pub fn open(config_dir: &Path) -> Self {
    let path = config_dir.join("route");
    let held = fs::read_to_string(&path)
      .ok()
      .map(|held| held.trim().to_owned())
      .filter(|held| held.starts_with('/'))
      .unwrap_or_else(|| DEFAULT_ROUTE.to_owned());
    Self {
      path,
      held: Mutex::new(held),
    }
  }

  pub fn get(&self) -> String {
    self.held.lock().unwrap().clone()
  }

  pub fn set(&self, route: String) {
    if !route.starts_with('/') {
      return;
    }
    let mut held = self.held.lock().unwrap();
    if *held == route {
      return;
    }
    *held = route;
    if let Some(parent) = self.path.parent()
      && let Err(error) = fs::create_dir_all(parent)
    {
      tracing::warn!(%error, path = %parent.display(), "the route directory could not be created");
      return;
    }
    if let Err(error) = fs::write(&self.path, held.as_bytes()) {
      tracing::warn!(%error, path = %self.path.display(), "the route could not be kept");
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn the_window_reopens_where_it_was_left() {
    let dir = tempfile::tempdir().expect("a scratch directory");

    let first = Route::open(dir.path());
    assert_eq!(first.get(), DEFAULT_ROUTE, "a first launch lands on the front page");
    first.set("/store/https%3A%2F%2Fapps.example/weather".to_owned());

    assert_eq!(
      Route::open(dir.path()).get(),
      "/store/https%3A%2F%2Fapps.example/weather",
      "a torn-down webview does not lose where the user was"
    );
  }

  #[test]
  fn a_route_that_is_not_a_path_is_refused() {
    let dir = tempfile::tempdir().expect("a scratch directory");
    let route = Route::open(dir.path());

    route.set("javascript:alert(1)".to_owned());
    route.set(String::new());

    assert_eq!(
      route.get(),
      DEFAULT_ROUTE,
      "only a path ever becomes the restored route"
    );
  }
}
