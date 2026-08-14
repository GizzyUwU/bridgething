use std::{
  collections::BTreeMap,
  path::PathBuf,
  pin::Pin,
  sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
  },
  task::{Context, Poll},
  time::Duration,
};

use anyhow::Result;
use bridgething_host_gateway::{
  chaos::ChaosConfig,
  ota::{PushRequest, PushShape, push},
  session,
};
use bridgething_test_harness::Harness;
use libbridgething::OtaKind;
use tokio::io::{AsyncRead, AsyncWrite, DuplexStream, ReadBuf};

const ZCK_ASSET: &str = "system.img.zck";
const ZCK_BYTES: usize = 1024 * 1024;
const SWU_BYTES: usize = 3 * 1024 * 1024;

const ARTIFACT_BYTES: usize = 512 * 1024;
const THROTTLE_BYTES_PER_SEC: u64 = 96 * 1024;
const KILL_FRACTION: f64 = 0.55;

struct CountedIo {
  inner: DuplexStream,
  written: Arc<AtomicU64>,
}

impl CountedIo {
  fn new(inner: DuplexStream) -> (Self, Arc<AtomicU64>) {
    let written = Arc::new(AtomicU64::new(0));
    (
      Self {
        inner,
        written: written.clone(),
      },
      written,
    )
  }
}

impl AsyncRead for CountedIo {
  fn poll_read(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<std::io::Result<()>> {
    Pin::new(&mut self.inner).poll_read(cx, buf)
  }
}

impl AsyncWrite for CountedIo {
  fn poll_write(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<std::io::Result<usize>> {
    match Pin::new(&mut self.inner).poll_write(cx, buf) {
      Poll::Ready(Ok(n)) => {
        self.written.fetch_add(n as u64, Ordering::Relaxed);
        Poll::Ready(Ok(n))
      }
      other => other,
    }
  }

  fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
    Pin::new(&mut self.inner).poll_flush(cx)
  }

  fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
    Pin::new(&mut self.inner).poll_shutdown(cx)
  }
}

fn artifact_fixture(dir: &std::path::Path) -> PathBuf {
  let body: Vec<u8> = (0..ARTIFACT_BYTES).map(|i| (i % 251) as u8).collect();
  let path = dir.join("daemon-artifact.bin");
  std::fs::write(&path, &body).unwrap();
  path
}

fn daemon_push(artifact: PathBuf) -> PushRequest {
  PushRequest {
    kind: OtaKind::Daemon,
    artifact,
    shape: PushShape::Whole,
    update_url_base: None,
    zcks: BTreeMap::new(),
    version: Some("9.9.9".into()),
  }
}

fn throttled() -> ChaosConfig {
  ChaosConfig {
    throttle_bytes_per_sec: Some(THROTTLE_BYTES_PER_SEC),
    ..Default::default()
  }
}

async fn wait_for_written(counter: &AtomicU64, at_least: u64, deadline: Duration) {
  let started = std::time::Instant::now();
  while counter.load(Ordering::Relaxed) < at_least {
    assert!(
      started.elapsed() < deadline,
      "link only carried {} of the {} bytes we waited for",
      counter.load(Ordering::Relaxed),
      at_least,
    );
    tokio::time::sleep(Duration::from_millis(10)).await;
  }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_daemon_ota_resumes_across_a_device_power_cut() -> Result<()> {
  let scratch = tempfile::tempdir()?;
  let artifact = artifact_fixture(scratch.path());
  let harness = Harness::start().await?;

  let (io, sent_before) = CountedIo::new(harness.connect_android_io().await?);
  let session_a = session::from_io(io, throttled()).await?;
  let push_artifact = artifact.clone();
  let interrupted = tokio::spawn(async move { push(&session_a, daemon_push(push_artifact)).await });

  let kill_at = (ARTIFACT_BYTES as f64 * KILL_FRACTION) as u64;
  wait_for_written(&sent_before, kill_at, Duration::from_secs(30)).await;

  let harness = harness.restart().await?;
  let _ = tokio::time::timeout(Duration::from_secs(30), interrupted)
    .await
    .expect("the interrupted push must terminate once the daemon is gone");

  let (io, sent_after) = CountedIo::new(harness.connect_android_io().await?);
  let session_b = session::from_io(io, throttled()).await?;
  push(&session_b, daemon_push(artifact)).await?;

  let before = sent_before.load(Ordering::Relaxed);
  let after = sent_after.load(Ordering::Relaxed);
  let size = ARTIFACT_BYTES as u64;
  assert!(
    after < size * 6 / 10,
    "the retry must resume, not restart: first attempt carried {before}, retry carried {after} of a {size}-byte artifact",
  );
  Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn a_daemon_ota_resumes_after_the_companion_dies_mid_stream() -> Result<()> {
  let scratch = tempfile::tempdir()?;
  let artifact = artifact_fixture(scratch.path());
  let harness = Harness::start().await?;

  let (io, sent_before) = CountedIo::new(harness.connect_android_io().await?);
  let session_a = session::from_io(io, throttled()).await?;
  let push_artifact = artifact.clone();
  let interrupted = tokio::spawn(async move { push(&session_a, daemon_push(push_artifact)).await });

  let kill_at = (ARTIFACT_BYTES as f64 * KILL_FRACTION) as u64;
  wait_for_written(&sent_before, kill_at, Duration::from_secs(30)).await;
  interrupted.abort();
  let _ = interrupted.await;

  let (io, sent_after) = CountedIo::new(harness.connect_android_io().await?);
  let session_b = session::from_io(io, throttled()).await?;
  push(&session_b, daemon_push(artifact)).await?;

  let after = sent_after.load(Ordering::Relaxed);
  let size = ARTIFACT_BYTES as u64;
  assert!(
    after < size * 6 / 10,
    "the retry after companion death must resume: retry carried {after} of a {size}-byte artifact",
  );
  Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn repeated_power_cuts_still_converge_with_bounded_total_transfer() -> Result<()> {
  let scratch = tempfile::tempdir()?;
  let artifact = artifact_fixture(scratch.path());
  let mut harness = Harness::start().await?;

  let size = ARTIFACT_BYTES as u64;
  let mut total_sent = 0u64;
  let mut completed = false;
  let cut_points = [0.25f64, 0.55, 0.85];

  for fraction in cut_points {
    let (io, sent) = CountedIo::new(harness.connect_android_io().await?);
    let session = session::from_io(io, throttled()).await?;
    let push_artifact = artifact.clone();
    let attempt = tokio::spawn(async move { push(&session, daemon_push(push_artifact)).await });

    let kill_at = ((size as f64 * fraction) as u64).saturating_sub(total_sent);
    let started = std::time::Instant::now();
    let mut finished_early = false;
    while sent.load(Ordering::Relaxed) < kill_at && started.elapsed() < Duration::from_secs(30) {
      if attempt.is_finished() {
        finished_early = true;
        break;
      }
      tokio::time::sleep(Duration::from_millis(10)).await;
    }

    harness = harness.restart().await?;
    let outcome = tokio::time::timeout(Duration::from_secs(30), attempt)
      .await
      .expect("attempt must terminate after the power cut");
    let attempt_sent = sent.load(Ordering::Relaxed);
    eprintln!("cut at {fraction}: attempt carried {attempt_sent} bytes (kill threshold {kill_at})");
    total_sent += attempt_sent;
    if finished_early && matches!(outcome, Ok(Ok(()))) {
      completed = true;
      break;
    }
  }

  if !completed {
    let (io, sent) = CountedIo::new(harness.connect_android_io().await?);
    let session = session::from_io(io, throttled()).await?;
    push(&session, daemon_push(artifact)).await?;
    total_sent += sent.load(Ordering::Relaxed);
  }

  assert!(
    total_sent < size * 18 / 10,
    "across three power cuts and a final attempt, {total_sent} bytes crossed the link for a {size}-byte artifact; \
     resume must bound re-transfer to roughly window-loss, not restart cost",
  );
  Ok(())
}

fn zck_byte(i: usize) -> u8 {
  ((i * 7 + 13) % 251) as u8
}

fn multipart_payloads(body: &[u8]) -> Vec<Vec<u8>> {
  let marker = b"\r\n--bridgething-ota-range-boundary";
  let mut payloads = Vec::new();
  let mut segments = Vec::new();
  let mut cursor = 0;
  while let Some(hit) = body[cursor..]
    .windows(marker.len())
    .position(|w| w == marker)
    .map(|p| p + cursor)
  {
    segments.push(&body[cursor..hit]);
    cursor = hit + marker.len();
  }
  for segment in segments {
    let Some(split) = segment.windows(4).position(|w| w == b"\r\n\r\n") else {
      continue;
    };
    payloads.push(segment[split + 4..].to_vec());
  }
  payloads
}

fn image_fixtures(dir: &std::path::Path) -> (PathBuf, PathBuf, Vec<u8>) {
  let swu: Vec<u8> = (0..SWU_BYTES).map(|i| (i % 253) as u8).collect();
  let swu_path = dir.join("update.swu");
  std::fs::write(&swu_path, &swu).unwrap();
  let zck: Vec<u8> = (0..ZCK_BYTES).map(zck_byte).collect();
  let zck_path = dir.join(ZCK_ASSET);
  std::fs::write(&zck_path, &zck).unwrap();
  (swu_path, zck_path, zck)
}

fn image_push(swu: PathBuf, zck: PathBuf) -> PushRequest {
  PushRequest {
    kind: OtaKind::Image,
    artifact: swu,
    shape: PushShape::Whole,
    update_url_base: Some("http://127.0.0.1:8893".into()),
    zcks: BTreeMap::from([(ZCK_ASSET.to_string(), zck)]),
    version: None,
  }
}

async fn get_range(client: &reqwest::Client, base: std::net::SocketAddr, start: usize, end: usize) -> Result<Vec<u8>> {
  let response = client
    .get(format!("http://{base}/{ZCK_ASSET}"))
    .header("Range", format!("bytes={start}-{end}"))
    .send()
    .await?;
  anyhow::ensure!(
    response.status() == reqwest::StatusCode::PARTIAL_CONTENT,
    "range GET answered {}: {}",
    response.status(),
    response.text().await.unwrap_or_default(),
  );
  Ok(response.bytes().await?.to_vec())
}

async fn get_range_when_active(
  client: &reqwest::Client,
  base: std::net::SocketAddr,
  start: usize,
  end: usize,
) -> Result<Vec<u8>> {
  let deadline = std::time::Instant::now() + Duration::from_secs(20);
  loop {
    match get_range(client, base, start, end).await {
      Ok(body) => return Ok(body),
      Err(_) if std::time::Instant::now() < deadline => {
        tokio::time::sleep(Duration::from_millis(250)).await;
      }
      Err(err) => return Err(err),
    }
  }
}

#[tokio::test(flavor = "multi_thread")]
async fn image_delta_range_pulls_replay_from_the_cache_after_a_power_cut() -> Result<()> {
  let scratch = tempfile::tempdir()?;
  let (swu_path, zck_path, zck) = image_fixtures(scratch.path());
  let harness = Harness::start().await?;
  let client = reqwest::Client::new();

  let (io, _sent_before) = CountedIo::new(harness.connect_android_io().await?);
  let session_a = session::from_io(io, throttled()).await?;
  let (swu_a, zck_a) = (swu_path.clone(), zck_path.clone());
  let attempt_a = tokio::spawn(async move { push(&session_a, image_push(swu_a, zck_a)).await });

  let proxy = harness
    .range_proxy_addr()
    .expect("headless daemon binds the range proxy");
  let windows = [(4096usize, 69631usize), (262144, 327679), (524288, 589823)];
  for (start, end) in windows {
    let body = get_range_when_active(&client, proxy, start, end).await?;
    assert_eq!(body, &zck[start..=end], "range {start}-{end} served wrong bytes");
  }

  let harness = harness.restart().await?;
  let _ = tokio::time::timeout(Duration::from_secs(30), attempt_a)
    .await
    .expect("the interrupted push must terminate once the daemon is gone");

  let (io, sent_after) = CountedIo::new(harness.connect_android_io().await?);
  let session_b = session::from_io(io, throttled()).await?;
  let (swu_b, zck_b) = (swu_path.clone(), zck_path.clone());
  let attempt_b = tokio::spawn(async move { push(&session_b, image_push(swu_b, zck_b)).await });

  let proxy = harness
    .range_proxy_addr()
    .expect("restarted daemon binds the range proxy");
  let (marker, first_body) = {
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    loop {
      let marker = sent_after.load(Ordering::Relaxed);
      match get_range(&client, proxy, windows[0].0, windows[0].1).await {
        Ok(body) => break (marker, body),
        Err(_) if std::time::Instant::now() < deadline => {
          tokio::time::sleep(Duration::from_millis(250)).await;
        }
        Err(err) => return Err(err),
      }
    }
  };
  assert_eq!(first_body, &zck[windows[0].0..=windows[0].1]);
  for (start, end) in &windows[1..] {
    let body = get_range(&client, proxy, *start, *end).await?;
    assert_eq!(
      body,
      &zck[*start..=*end],
      "cached range {start}-{end} served wrong bytes"
    );
  }
  let replayed_link_bytes = sent_after.load(Ordering::Relaxed) - marker;

  let payload: usize = windows.iter().map(|(s, e)| e - s + 1).sum();
  assert!(
    (replayed_link_bytes as usize) < payload / 2,
    "replaying {payload} bytes of already-fetched ranges moved {replayed_link_bytes} bytes over the link; \
     the cache must serve them locally after a power cut",
  );

  let fresh = (700 * 1024usize, 700 * 1024 + 32767);
  let body = get_range(&client, proxy, fresh.0, fresh.1).await?;
  assert_eq!(
    body,
    &zck[fresh.0..=fresh.1],
    "a gap range must still fetch from the companion"
  );

  attempt_b.abort();
  let _ = attempt_b.await;
  Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn a_many_hundreds_of_ranges_request_survives_the_wire() -> Result<()> {
  let scratch = tempfile::tempdir()?;
  let (swu_path, zck_path, zck) = image_fixtures(scratch.path());
  let harness = Harness::start().await?;
  let client = reqwest::Client::new();

  let (io, _sent) = CountedIo::new(harness.connect_android_io().await?);
  let session = session::from_io(io, throttled()).await?;
  let attempt = tokio::spawn(async move { push(&session, image_push(swu_path, zck_path)).await });

  let proxy = harness
    .range_proxy_addr()
    .expect("headless daemon binds the range proxy");
  let _ = get_range_when_active(&client, proxy, 0, 511).await?;

  let parts: Vec<String> = (0..300)
    .map(|i| {
      let start = 1024 + i * 2048;
      format!("{start}-{}", start + 511)
    })
    .collect();
  let response = client
    .get(format!("http://{proxy}/{ZCK_ASSET}"))
    .header("Range", format!("bytes={}", parts.join(",")))
    .send()
    .await?;
  anyhow::ensure!(
    response.status() == reqwest::StatusCode::PARTIAL_CONTENT,
    "300-range GET answered {}",
    response.status(),
  );
  let body = response.bytes().await?;

  let payloads = multipart_payloads(&body);
  assert_eq!(payloads.len(), 300, "every requested range comes back as a part");
  for (i, payload) in payloads.iter().enumerate() {
    let start = 1024 + i * 2048;
    assert_eq!(
      payload.as_slice(),
      &zck[start..start + 512],
      "part {i} (zck offset {start}) carries the wrong bytes",
    );
  }

  attempt.abort();
  let _ = attempt.await;
  Ok(())
}
