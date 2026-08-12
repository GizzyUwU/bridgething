use std::sync::{Arc, Mutex};

use bridgething_companion::dispatch::webapp::{DEFAULT_HERO_PX, DEFAULT_THUMB_PX, WebappDispatcher, WebappObserver};
use bridgething_gateway::WebappHandler;
use libbridgething::{
  ArtProfile, WebappInfo,
  gateway::{WebappActiveChanged, WebappDocChanged},
};
use uuid::Uuid;

use crate::fakes::{FakeProvider, FakeRegistry};

const DEVICE: &str = "webapp-device";

#[derive(Default)]
struct RecordedWebapps {
  docs: Mutex<Vec<(String, WebappDocChanged)>>,
  installed: Mutex<Vec<(String, WebappInfo)>>,
  active: Mutex<Vec<(String, WebappActiveChanged)>>,
}

impl WebappObserver for RecordedWebapps {
  fn doc_changed(&self, device_id: &str, changed: WebappDocChanged) {
    self.docs.lock().unwrap().push((device_id.to_owned(), changed));
  }

  fn installed(&self, device_id: &str, info: WebappInfo) {
    self.installed.lock().unwrap().push((device_id.to_owned(), info));
  }

  fn active_changed(&self, device_id: &str, changed: WebappActiveChanged) {
    self.active.lock().unwrap().push((device_id.to_owned(), changed));
  }
}

#[tokio::test]
async fn an_active_webapp_change_pushes_its_art_profile_to_every_provider() {
  let first = FakeProvider::bare("spotify");
  let second = FakeProvider::bare("applemusic");
  let observer = Arc::new(RecordedWebapps::default());
  let dispatch = WebappDispatcher::new(
    FakeRegistry::of(vec![first.clone(), second.clone()]),
    observer.clone(),
    DEVICE,
  );

  dispatch
    .active_changed(WebappActiveChanged {
      id: Some(Uuid::now_v7()),
      name: Some("nowplaying".into()),
      art: Some(ArtProfile {
        hero_px: 512,
        thumb_px: 128,
      }),
    })
    .await
    .expect("an event never refuses");

  assert_eq!(*first.art_profile.lock().unwrap(), Some((512, 128)));
  assert_eq!(*second.art_profile.lock().unwrap(), Some((512, 128)));
  assert_eq!(observer.active.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn an_active_webapp_with_no_art_profile_falls_back_to_the_defaults() {
  let provider = FakeProvider::bare("spotify");
  let dispatch = WebappDispatcher::new(
    FakeRegistry::with(provider.clone()),
    Arc::new(RecordedWebapps::default()),
    DEVICE,
  );

  dispatch
    .active_changed(WebappActiveChanged {
      id: None,
      name: None,
      art: None,
    })
    .await
    .expect("an event never refuses");

  assert_eq!(
    *provider.art_profile.lock().unwrap(),
    Some((DEFAULT_HERO_PX, DEFAULT_THUMB_PX))
  );
}

#[tokio::test]
async fn a_webapp_doc_change_reaches_the_observer_intact() {
  let observer = Arc::new(RecordedWebapps::default());
  let dispatch = WebappDispatcher::new(FakeRegistry::empty(), observer.clone(), DEVICE);
  let webapp_id = Uuid::now_v7();

  dispatch
    .doc_changed(WebappDocChanged {
      id: webapp_id,
      key: "theme".into(),
      value: Some("dark".into()),
    })
    .await
    .expect("an event never refuses");

  let docs = observer.docs.lock().unwrap();
  assert_eq!(docs.len(), 1);
  assert_eq!(docs[0].0, DEVICE, "the observer is told which peer reported the change");
  assert_eq!(docs[0].1.id, webapp_id);
  assert_eq!(docs[0].1.key, "theme");
  assert_eq!(docs[0].1.value.as_deref(), Some("dark"));
}
