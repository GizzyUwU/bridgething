use std::{
  collections::{HashMap, HashSet},
  net::SocketAddr,
  sync::Arc,
  time::{Duration, Instant},
};

use bridgething_delivery::discovery::Endpoint;
use tokio::task::JoinSet;

use crate::shell::Shell;

const FIRST_RETRY: Duration = Duration::from_secs(1);
const RETRY_CAP: Duration = Duration::from_secs(30);
const HELD: Duration = Duration::from_secs(30);

pub fn spawn(shell: Arc<Shell>, discovered: impl Fn() -> Vec<Endpoint> + Send + Sync + 'static) {
  tauri::async_runtime::spawn(drive(shell, discovered));
}

async fn drive(shell: Arc<Shell>, discovered: impl Fn() -> Vec<Endpoint>) {
  let wake = shell.wake();
  let mut schedule = HashMap::new();
  loop {
    match sweep(&shell, &discovered(), &mut schedule).await {
      Some(due) => {
        tokio::select! {
          () = wake.notified() => {}
          () = tokio::time::sleep(due) => {}
        }
      }
      None => wake.notified().await,
    }
  }
}

struct Attempt {
  backoff: Duration,
  next: Instant,
  opened: Option<Instant>,
}

impl Attempt {
  fn fresh() -> Self {
    Self {
      backoff: FIRST_RETRY,
      next: Instant::now(),
      opened: None,
    }
  }

  fn gone(&mut self) {
    let Some(opened) = self.opened.take() else {
      return;
    };
    self.backoff = if opened.elapsed() >= HELD {
      FIRST_RETRY
    } else {
      (self.backoff * 2).min(RETRY_CAP)
    };
    self.next = Instant::now() + self.backoff;
  }

  fn refused(&mut self) {
    self.backoff = (self.backoff * 2).min(RETRY_CAP);
    self.next = Instant::now() + self.backoff;
  }

  fn held(&mut self) {
    self.opened = Some(Instant::now());
  }
}

async fn sweep(shell: &Arc<Shell>, found: &[Endpoint], schedule: &mut HashMap<String, Attempt>) -> Option<Duration> {
  let discovered: Vec<String> = found.iter().map(|endpoint| endpoint.url.clone()).collect();
  let wanted = shell.auto_connect_targets(&discovered);
  schedule.retain(|url, _| wanted.contains(url));

  let linked = shell.linked_ids();
  let mut due = Vec::new();
  for url in wanted {
    if linked.contains(&url) {
      continue;
    }
    let attempt = schedule.entry(url.clone()).or_insert_with(Attempt::fresh);
    attempt.gone();
    if attempt.next <= Instant::now() {
      due.push(url);
    }
  }

  let addrs = resolved(due.iter().chain(linked.iter()).cloned().collect()).await;
  let held: Vec<Vec<SocketAddr>> = linked
    .iter()
    .map(|url| addrs.get(url).cloned().unwrap_or_default())
    .collect();
  let known: HashSet<String> = shell.known_devices().into_iter().map(|device| device.url).collect();

  let mut dials = JoinSet::new();
  for url in distinct(&due, &addrs, &held, &known) {
    let label = found
      .iter()
      .find(|endpoint| endpoint.url == url)
      .map(|endpoint| endpoint.nickname.clone().unwrap_or_else(|| endpoint.host.clone()));
    let shell = Arc::clone(shell);
    dials.spawn(async move {
      let outcome = shell.dial(url.clone(), label).await;
      (url, outcome)
    });
  }

  while let Some(Ok((url, outcome))) = dials.join_next().await {
    let Some(attempt) = schedule.get_mut(&url) else {
      continue;
    };
    match outcome {
      Ok(_) => {
        tracing::info!(%url, "an attached device is linked");
        attempt.held();
      }
      Err(error) => {
        tracing::debug!(%url, %error, "an attached device is not answering yet");
        attempt.refused();
      }
    }
  }

  let now = Instant::now();
  schedule
    .values()
    .filter(|attempt| attempt.next > now)
    .map(|attempt| attempt.next - now)
    .min()
}

fn distinct(
  due: &[String],
  addrs: &HashMap<String, Vec<SocketAddr>>,
  held: &[Vec<SocketAddr>],
  known: &HashSet<String>,
) -> Vec<String> {
  let of = |url: &String| addrs.get(url).map_or(&[][..], Vec::as_slice);
  let overlaps = |a: &[SocketAddr], b: &[SocketAddr]| a.iter().any(|addr| b.contains(addr));
  let mut picked: Vec<String> = Vec::new();
  for url in due {
    if held.iter().any(|line| overlaps(of(url), line)) {
      continue;
    }
    match picked.iter().position(|winner| overlaps(of(url), of(winner))) {
      Some(seat) if known.contains(url) && !known.contains(&picked[seat]) => picked[seat] = url.clone(),
      Some(_) => {}
      None => picked.push(url.clone()),
    }
  }
  picked
}

async fn resolved(urls: Vec<String>) -> HashMap<String, Vec<SocketAddr>> {
  let mut lookups = JoinSet::new();
  for url in urls {
    lookups.spawn(async move {
      let addrs = match authority(&url) {
        Some(authority) => tokio::net::lookup_host(authority)
          .await
          .map(Iterator::collect)
          .unwrap_or_default(),
        None => Vec::new(),
      };
      (url, addrs)
    });
  }
  let mut out = HashMap::new();
  while let Some(Ok((url, addrs))) = lookups.join_next().await {
    out.insert(url, addrs);
  }
  out
}

fn authority(url: &str) -> Option<&str> {
  let rest = url.strip_prefix("ws://").or_else(|| url.strip_prefix("wss://"))?;
  let authority = rest.split('/').next().unwrap_or(rest);
  let port = &authority[authority.rfind(':')? + 1..];
  (!port.is_empty() && port.bytes().all(|byte| byte.is_ascii_digit())).then_some(authority)
}

#[cfg(test)]
mod tests {
  use super::*;

  fn addr(tail: u8) -> SocketAddr {
    SocketAddr::from(([10, 42, 0, tail], 8892))
  }

  fn table(rows: &[(&str, &[SocketAddr])]) -> HashMap<String, Vec<SocketAddr>> {
    rows
      .iter()
      .map(|(url, addrs)| ((*url).to_owned(), addrs.to_vec()))
      .collect()
  }

  #[test]
  fn two_names_for_one_device_are_one_dial_and_a_remembered_name_wins_it() {
    let due = vec![
      "ws://bridgething-abc.local:8892/".to_owned(),
      "ws://bridgething.local:8892/".to_owned(),
    ];
    let addrs = table(&[
      ("ws://bridgething-abc.local:8892/", &[addr(2)][..]),
      ("ws://bridgething.local:8892/", &[addr(2)][..]),
    ]);
    let known = HashSet::from(["ws://bridgething.local:8892/".to_owned()]);

    assert_eq!(
      distinct(&due, &addrs, &[], &known),
      vec!["ws://bridgething.local:8892/".to_owned()],
      "one device behind two names is dialed once, by the name already on file"
    );
  }

  #[test]
  fn two_devices_are_two_dials() {
    let due = vec![
      "ws://bridgething-abc.local:8892/".to_owned(),
      "ws://bridgething-def.local:8892/".to_owned(),
    ];
    let addrs = table(&[
      ("ws://bridgething-abc.local:8892/", &[addr(2)][..]),
      ("ws://bridgething-def.local:8892/", &[addr(10)][..]),
    ]);

    assert_eq!(
      distinct(&due, &addrs, &[], &HashSet::new()).len(),
      2,
      "every attached device gets its own dial"
    );
  }

  #[test]
  fn a_second_name_for_a_device_already_linked_is_left_alone() {
    let due = vec!["ws://bridgething.local:8892/".to_owned()];
    let addrs = table(&[("ws://bridgething.local:8892/", &[addr(2)][..])]);
    let held = vec![vec![addr(2)]];

    assert!(
      distinct(&due, &addrs, &held, &HashSet::new()).is_empty(),
      "a device holding a link is not dialed again under another name"
    );
  }

  #[test]
  fn a_name_that_does_not_resolve_is_still_dialed() {
    let due = vec![
      "ws://bridgething-abc.local:8892/".to_owned(),
      "ws://bridgething-def.local:8892/".to_owned(),
    ];
    let addrs = table(&[
      ("ws://bridgething-abc.local:8892/", &[][..]),
      ("ws://bridgething-def.local:8892/", &[][..]),
    ]);

    assert_eq!(
      distinct(&due, &addrs, &[], &HashSet::new()).len(),
      2,
      "an unresolvable name cannot be proven a duplicate, so the dial decides"
    );
  }

  #[test]
  fn the_authority_is_the_dialable_host_and_port() {
    assert_eq!(
      authority("ws://bridgething.local:8892/"),
      Some("bridgething.local:8892")
    );
    assert_eq!(authority("ws://127.0.0.1:8892/"), Some("127.0.0.1:8892"));
    assert_eq!(
      authority("ws://bridgething.local/"),
      None,
      "no port, nothing to resolve"
    );
    assert_eq!(authority("http://x:1/"), None, "not a gateway url shape");
  }
}
