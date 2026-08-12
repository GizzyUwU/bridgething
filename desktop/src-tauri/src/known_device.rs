use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::store::{JsonFile, stored};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnownDevice {
  pub url: String,
  pub name: String,
  pub auto_connect: bool,
  pub last_connected_at: Option<String>,
}

pub struct KnownDevices(JsonFile<Vec<KnownDevice>>);

impl KnownDevices {
  pub fn open(config_dir: &Path) -> Self {
    let path = config_dir.join("known-devices.json");
    let held = stored(&path).unwrap_or_default();
    Self(JsonFile::new(path, "known device list", held))
  }

  pub fn list(&self) -> Vec<KnownDevice> {
    self.0.read(|held| held.clone())
  }

  pub fn wanted(&self, discovered: &[String]) -> Vec<String> {
    self.0.read(|held| {
      let mut out: Vec<String> = held
        .iter()
        .filter(|known| known.auto_connect)
        .map(|known| known.url.clone())
        .collect();
      for url in discovered {
        let declined = held.iter().any(|known| &known.url == url && !known.auto_connect);
        if !declined && !out.contains(url) {
          out.push(url.clone());
        }
      }
      out
    })
  }

  pub fn record(&self, url: &str, label: Option<&str>) {
    let at = Some(chrono::Utc::now().to_rfc3339());
    self
      .0
      .write(|held| match held.iter_mut().find(|known| known.url == url) {
        Some(known) => {
          if let Some(label) = label {
            known.name = label.to_owned();
          }
          known.last_connected_at = at;
        }
        None => held.push(KnownDevice {
          url: url.to_owned(),
          name: label.unwrap_or("bridgething daemon").to_owned(),
          auto_connect: true,
          last_connected_at: at,
        }),
      });
  }

  pub fn set_auto_connect(&self, url: &str, enabled: bool) -> bool {
    let moves = self.0.read(|held| {
      held
        .iter()
        .any(|known| known.url == url && known.auto_connect != enabled)
    });
    if !moves {
      return false;
    }
    self.0.write(|held| {
      if let Some(known) = held.iter_mut().find(|known| known.url == url) {
        known.auto_connect = enabled;
      }
    });
    true
  }

  pub fn forget(&self, url: &str) -> bool {
    self.0.write(|held| {
      let before = held.len();
      held.retain(|known| known.url != url);
      held.len() != before
    })
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  const URL: &str = "ws://bridgething.local:8892/";

  #[test]
  fn a_device_that_came_up_once_is_dialed_again_at_the_next_launch() {
    let dir = tempfile::tempdir().expect("a scratch directory");

    let first = KnownDevices::open(dir.path());
    assert!(first.wanted(&[]).is_empty(), "a fresh host dials nothing on its own");
    first.record(URL, None);

    let reopened = KnownDevices::open(dir.path());
    assert_eq!(
      reopened.wanted(&[]),
      vec![URL.to_owned()],
      "a device that connected once is remembered across the process that connected it"
    );
    let held = reopened.list();
    assert_eq!(held[0].name, "bridgething daemon");
    assert!(held[0].last_connected_at.is_some(), "and it says when it last answered");
  }

  #[test]
  fn a_discovered_daemon_is_dialed_before_anyone_has_ever_connected_to_it() {
    let dir = tempfile::tempdir().expect("a scratch directory");

    let devices = KnownDevices::open(dir.path());
    let fresh = "ws://bridgething-abc123.local:8892/".to_owned();
    assert_eq!(
      devices.wanted(std::slice::from_ref(&fresh)),
      vec![fresh.clone()],
      "showing up on the network is enough to be dialed"
    );

    devices.record(URL, None);
    assert_eq!(
      devices.wanted(&[fresh.clone(), URL.to_owned()]),
      vec![URL.to_owned(), fresh],
      "remembered devices come first and a discovered duplicate of one is not listed twice"
    );
  }

  #[test]
  fn a_device_the_user_turned_off_stays_off() {
    let dir = tempfile::tempdir().expect("a scratch directory");

    let devices = KnownDevices::open(dir.path());
    devices.record(URL, None);
    assert!(devices.set_auto_connect(URL, false), "the choice is a change");
    assert!(!devices.set_auto_connect(URL, false), "and saying it twice is not");

    devices.record(URL, None);
    assert!(
      KnownDevices::open(dir.path()).wanted(&[]).is_empty(),
      "a later connect refreshes the entry without turning auto-connect back on"
    );
    assert!(
      devices.wanted(&[URL.to_owned()]).is_empty(),
      "and announcing itself on the network does not override the choice"
    );
  }

  #[test]
  fn a_label_names_the_entry_and_its_absence_keeps_the_name_already_there() {
    let dir = tempfile::tempdir().expect("a scratch directory");

    let devices = KnownDevices::open(dir.path());
    devices.record(URL, Some("Kitchen Thing"));
    assert_eq!(devices.list()[0].name, "Kitchen Thing");

    devices.record(URL, None);
    assert_eq!(
      devices.list()[0].name,
      "Kitchen Thing",
      "a nameless reconnect does not erase what discovery learned"
    );
  }

  #[test]
  fn a_device_nobody_ever_reached_is_not_a_device_at_all() {
    let dir = tempfile::tempdir().expect("a scratch directory");

    let devices = KnownDevices::open(dir.path());
    assert!(!devices.set_auto_connect(URL, true), "there is nothing to turn on");
    assert!(!devices.forget(URL), "and nothing to forget");
    assert!(devices.list().is_empty());
  }

  #[test]
  fn a_forgotten_device_is_gone_from_disk() {
    let dir = tempfile::tempdir().expect("a scratch directory");

    let devices = KnownDevices::open(dir.path());
    devices.record(URL, None);
    assert!(devices.forget(URL));

    assert!(
      KnownDevices::open(dir.path()).list().is_empty(),
      "forgetting is flushed as eagerly as recording"
    );
  }
}
