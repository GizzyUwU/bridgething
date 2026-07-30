use std::{
  io::{ErrorKind, Read, Write},
  os::unix::net::{UnixListener, UnixStream},
  path::{Path, PathBuf},
};

use bridgething_wakeword::{WakeWord, features::CHUNK_SAMPLES};

const READ_SAMPLES: usize = CHUNK_SAMPLES;
const DEFAULT_THRESHOLD: f32 = 0.5;

struct Args {
  socket: PathBuf,
  models: PathBuf,
  phrase: PathBuf,
  threshold: f32,
}

fn args() -> Result<Args, String> {
  let mut raw = std::env::args().skip(1);
  let mut next = |what: &str| raw.next().ok_or_else(|| format!("missing <{what}>"));
  let socket = PathBuf::from(next("socket")?);
  let models = PathBuf::from(next("models-dir")?);
  let phrase = PathBuf::from(next("phrase-model")?);
  let threshold = match raw.next() {
    Some(value) => value.parse().map_err(|_| format!("bad threshold: {value}"))?,
    None => DEFAULT_THRESHOLD,
  };
  Ok(Args {
    socket,
    models,
    phrase,
    threshold,
  })
}

fn bind(path: &Path) -> std::io::Result<UnixListener> {
  match UnixStream::connect(path) {
    Ok(_) => {
      return Err(std::io::Error::new(
        ErrorKind::AddrInUse,
        format!("{} is already served", path.display()),
      ));
    }
    Err(err) if err.kind() == ErrorKind::ConnectionRefused => {
      let _ = std::fs::remove_file(path);
    }
    Err(_) => {}
  }
  UnixListener::bind(path)
}

fn serve(stream: &mut UnixStream, detector: &mut WakeWord) -> std::io::Result<()> {
  let mut bytes = vec![0u8; READ_SAMPLES * 2];
  let mut samples = Vec::with_capacity(READ_SAMPLES);
  let mut filled = 0usize;

  loop {
    match stream.read(&mut bytes[filled..]) {
      Ok(0) => return Ok(()),
      Ok(read) => filled += read,
      Err(err) if err.kind() == ErrorKind::Interrupted => continue,
      Err(err) => return Err(err),
    }

    let whole = filled / 2;
    samples.clear();
    samples.extend(
      bytes[..whole * 2]
        .chunks_exact(2)
        .map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0),
    );
    bytes.copy_within(whole * 2..filled, 0);
    filled -= whole * 2;

    match detector.push(&samples) {
      Ok(Some(hit)) => {
        tracing::info!(score = hit.score, at_sample = hit.at_sample, "wake word");
        writeln!(stream, "{{\"score\":{:.4},\"at_sample\":{}}}", hit.score, hit.at_sample)?;
        stream.flush()?;
      }
      Ok(None) => {}
      Err(err) => tracing::warn!("inference failed: {err}"),
    }
  }
}

fn main() {
  tracing_subscriber::fmt()
    .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
    .init();

  let args = match args() {
    Ok(args) => args,
    Err(err) => {
      eprintln!("{err}\nusage: bridgething-wakeword <socket> <models-dir> <phrase-model> [threshold]");
      std::process::exit(2);
    }
  };

  let mut detector = match WakeWord::new(&args.models, &args.phrase, args.threshold) {
    Ok(detector) => detector,
    Err(err) => {
      tracing::error!("could not load wake word: {err}");
      std::process::exit(1);
    }
  };

  let listener = match bind(&args.socket) {
    Ok(listener) => listener,
    Err(err) => {
      tracing::error!("could not bind {}: {err}", args.socket.display());
      std::process::exit(1);
    }
  };
  tracing::info!(
    socket = %args.socket.display(),
    threshold = args.threshold,
    "wake word listening"
  );

  for incoming in listener.incoming() {
    let mut stream = match incoming {
      Ok(stream) => stream,
      Err(err) => {
        tracing::warn!("accept failed: {err}");
        continue;
      }
    };
    tracing::info!("daemon attached");
    detector.reset();
    if let Err(err) = serve(&mut stream, &mut detector) {
      tracing::warn!("connection ended: {err}");
    } else {
      tracing::info!("daemon detached");
    }
  }
}
