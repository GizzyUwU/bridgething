use std::{
  fs::{self, File, OpenOptions},
  io,
  path::{Path, PathBuf},
  sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
    mpsc,
  },
  time::{Duration, Instant},
};

const ENV_TRACE_DIR: &str = "BRIDGETHING_TRACE_DIR";
const ENV_TRACE_FILTER: &str = "BRIDGETHING_TRACE_FILTER";
const TRACE_FILE_DIRECTIVES: &str =
  "bridgething=trace,libbridgething=trace,bridgething_iap2=trace,bridgething_mfi=trace";
const TRACE_FILE_NAME: &str = "bridgething.log";
const TRACE_FILE_CAP_BYTES: u64 = 32 * 1024 * 1024;
const TRACE_ROTATED_KEEP: usize = 3;
const TRACE_SYNC_INTERVAL: Duration = Duration::from_secs(2);
const TRACE_CHANNEL_CAP: usize = 65_536;

pub fn init_logger(tap: crate::state::LogTapLayer) {
  use tracing::metadata::LevelFilter;
  use tracing_subscriber::{
    EnvFilter, Layer, filter::Directive, fmt, fmt::format::FmtSpan, prelude::__tracing_subscriber_SubscriberExt,
    util::SubscriberInitExt,
  };

  // directives for debug builds
  #[cfg(debug_assertions)]
  let default_directive = Directive::from(LevelFilter::TRACE);

  #[cfg(debug_assertions)]
  let filter_directives = if let Ok(filter) = std::env::var("RUST_LOG") {
    filter
  } else {
    "bridgething=trace,bridgething::ws::connection::send=debug,bridgething::net=debug,libbridgething=trace,bridgething_iap2=trace,bridgething_mfi=trace".to_string()
  };

  // directives for release builds
  #[cfg(not(debug_assertions))]
  let default_directive = Directive::from(LevelFilter::INFO);

  #[cfg(not(debug_assertions))]
  let filter_directives = if let Ok(filter) = std::env::var("RUST_LOG") {
    filter
  } else {
    "bridgething=info,libbridgething=info,bridgething_iap2=info,bridgething_mfi=info".to_string()
  };

  let make_filter = |directives: &str| {
    EnvFilter::builder()
      .with_default_directive(default_directive.clone())
      .parse_lossy(directives)
  };

  let file_directives = std::env::var(ENV_TRACE_FILTER).unwrap_or_else(|_| TRACE_FILE_DIRECTIVES.to_string());
  let file_layer = trace_file_writer().map(|writer| {
    fmt::layer()
      .with_ansi(false)
      .with_span_events(FmtSpan::CLOSE)
      .with_writer(writer)
      .with_filter(make_filter(&file_directives))
  });

  tracing_subscriber::registry()
    .with(
      fmt::layer()
        .with_span_events(FmtSpan::CLOSE)
        .with_filter(make_filter(&filter_directives)),
    )
    .with(tap.with_filter(make_filter(&filter_directives)))
    .with(file_layer)
    .init();

  tracing::debug!("initialized logger");
}

fn trace_file_writer() -> Option<TraceFileTx> {
  let dir = PathBuf::from(std::env::var_os(ENV_TRACE_DIR)?);
  if let Err(err) = fs::create_dir_all(&dir) {
    eprintln!("trace log dir {} unusable: {err}", dir.display());
    return None;
  }
  let worker = match TraceFileWorker::open(dir) {
    Ok(worker) => worker,
    Err(err) => {
      eprintln!("trace log file unusable: {err}");
      return None;
    }
  };
  let (tx, rx) = mpsc::sync_channel(TRACE_CHANNEL_CAP);
  let dropped = Arc::new(AtomicU64::new(0));
  let worker_dropped = dropped.clone();
  let spawned = std::thread::Builder::new()
    .name("trace-log".into())
    .spawn(move || worker.run(rx, worker_dropped));
  if spawned.is_err() {
    eprintln!("trace log worker thread failed to spawn");
    return None;
  }
  Some(TraceFileTx { tx, dropped })
}

#[derive(Clone)]
struct TraceFileTx {
  tx: mpsc::SyncSender<Vec<u8>>,
  dropped: Arc<AtomicU64>,
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for TraceFileTx {
  type Writer = TraceFileEventBuf;

  fn make_writer(&'a self) -> Self::Writer {
    TraceFileEventBuf {
      buf: Vec::new(),
      tx: self.tx.clone(),
      dropped: self.dropped.clone(),
    }
  }
}

struct TraceFileEventBuf {
  buf: Vec<u8>,
  tx: mpsc::SyncSender<Vec<u8>>,
  dropped: Arc<AtomicU64>,
}

impl io::Write for TraceFileEventBuf {
  fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
    self.buf.extend_from_slice(buf);
    Ok(buf.len())
  }

  fn flush(&mut self) -> io::Result<()> {
    Ok(())
  }
}

impl Drop for TraceFileEventBuf {
  fn drop(&mut self) {
    if !self.buf.is_empty() && self.tx.try_send(std::mem::take(&mut self.buf)).is_err() {
      self.dropped.fetch_add(1, Ordering::Relaxed);
    }
  }
}

struct TraceFileWorker {
  dir: PathBuf,
  file: File,
  written: u64,
  dirty: bool,
  last_sync: Instant,
}

impl TraceFileWorker {
  fn open(dir: PathBuf) -> io::Result<Self> {
    let file = OpenOptions::new()
      .create(true)
      .append(true)
      .open(dir.join(TRACE_FILE_NAME))?;
    let written = file.metadata()?.len();
    Ok(Self {
      dir,
      file,
      written,
      dirty: false,
      last_sync: Instant::now(),
    })
  }

  fn run(mut self, rx: mpsc::Receiver<Vec<u8>>, dropped: Arc<AtomicU64>) {
    loop {
      match rx.recv_timeout(TRACE_SYNC_INTERVAL) {
        Ok(event) => {
          let lost = dropped.swap(0, Ordering::Relaxed);
          if lost > 0 {
            self.append(format!("[trace log: {lost} events dropped under backpressure]\n").as_bytes());
          }
          self.append(&event);
          if self.last_sync.elapsed() >= TRACE_SYNC_INTERVAL {
            self.sync();
          }
        }
        Err(mpsc::RecvTimeoutError::Timeout) => {
          if self.dirty {
            self.sync();
          }
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => {
          self.sync();
          return;
        }
      }
    }
  }

  fn append(&mut self, bytes: &[u8]) {
    use io::Write as _;
    if self.written.saturating_add(bytes.len() as u64) > TRACE_FILE_CAP_BYTES {
      self.rotate();
    }
    if self.file.write_all(bytes).is_ok() {
      self.written += bytes.len() as u64;
      self.dirty = true;
    }
  }

  fn rotate(&mut self) {
    self.sync();
    let current = self.dir.join(TRACE_FILE_NAME);
    let _ = fs::remove_file(rotated_path(&self.dir, TRACE_ROTATED_KEEP));
    for n in (1..TRACE_ROTATED_KEEP).rev() {
      let _ = fs::rename(rotated_path(&self.dir, n), rotated_path(&self.dir, n + 1));
    }
    let _ = fs::rename(&current, rotated_path(&self.dir, 1));
    match OpenOptions::new().create(true).append(true).open(&current) {
      Ok(file) => {
        self.file = file;
        self.written = 0;
        self.dirty = false;
      }
      Err(_) => {
        self.written = 0;
      }
    }
  }

  fn sync(&mut self) {
    let _ = self.file.sync_data();
    self.dirty = false;
    self.last_sync = Instant::now();
  }
}

fn rotated_path(dir: &Path, n: usize) -> PathBuf {
  dir.join(format!("{TRACE_FILE_NAME}.{n}"))
}

pub async fn wait_for_signal() {
  use tokio::signal::{
    ctrl_c,
    unix::{SignalKind, signal},
  };

  let mut signal_terminate = signal(SignalKind::terminate()).expect("could not create signal handler");

  tokio::select! {
    _ = signal_terminate.recv() => tracing::info!("received SIGTERM, shutting down"),
    _ = ctrl_c() => tracing::info!("ctrl-c received, shutting down"),
  };
}

#[cfg(test)]
mod tests {
  use super::*;

  fn temp_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("bridgething-trace-test-{}", uuid::Uuid::now_v7()));
    fs::create_dir_all(&dir).unwrap();
    dir
  }

  fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap()
  }

  #[test]
  fn append_reopens_existing_file_without_truncating() {
    let dir = temp_dir();
    let mut worker = TraceFileWorker::open(dir.clone()).unwrap();
    worker.append(b"before reboot\n");
    worker.sync();
    drop(worker);

    let mut worker = TraceFileWorker::open(dir.clone()).unwrap();
    worker.append(b"after reboot\n");
    worker.sync();

    let contents = read(&dir.join(TRACE_FILE_NAME));
    assert!(contents.contains("before reboot"), "pre-reboot tail preserved");
    assert!(contents.contains("after reboot"));
    fs::remove_dir_all(&dir).unwrap();
  }

  #[test]
  fn rotation_shifts_files_and_caps_the_set() {
    let dir = temp_dir();
    let mut worker = TraceFileWorker::open(dir.clone()).unwrap();

    worker.append(b"gen 0\n");
    for generation in 1..=(TRACE_ROTATED_KEEP + 1) {
      worker.written = TRACE_FILE_CAP_BYTES;
      worker.append(format!("gen {generation}\n").as_bytes());
    }
    worker.sync();

    assert!(read(&dir.join(TRACE_FILE_NAME)).contains("gen 4"));
    assert!(read(&rotated_path(&dir, 1)).contains("gen 3"));
    assert!(read(&rotated_path(&dir, 2)).contains("gen 2"));
    assert!(read(&rotated_path(&dir, 3)).contains("gen 1"));
    assert!(
      !rotated_path(&dir, TRACE_ROTATED_KEEP + 1).exists(),
      "oldest generation is deleted, not shifted"
    );
    fs::remove_dir_all(&dir).unwrap();
  }

  #[test]
  fn worker_drains_channel_and_marks_backpressure_drops() {
    let dir = temp_dir();
    let worker = TraceFileWorker::open(dir.clone()).unwrap();
    let (tx, rx) = mpsc::sync_channel(TRACE_CHANNEL_CAP);
    let dropped = Arc::new(AtomicU64::new(0));
    dropped.store(2, Ordering::Relaxed);
    let handle = {
      let dropped = dropped.clone();
      std::thread::spawn(move || worker.run(rx, dropped))
    };

    tx.send(b"live event\n".to_vec()).unwrap();
    drop(tx);
    handle.join().unwrap();

    let contents = read(&dir.join(TRACE_FILE_NAME));
    assert!(contents.contains("2 events dropped"));
    assert!(contents.contains("live event"));
    assert_eq!(dropped.load(Ordering::Relaxed), 0, "drop counter consumed");
    fs::remove_dir_all(&dir).unwrap();
  }
}
