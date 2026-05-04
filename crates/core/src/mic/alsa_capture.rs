//! Synchronous ALSA capture worker. Lives on a regular OS thread so
//! the alsa-rs blocking reads stay off the tokio runtime; pushes
//! captured PCM frames into the manager via a tokio mpsc using
//! `blocking_send`. The cancel flag is the only graceful-stop path —
//! the worker checks it between frames and on every read error.

use std::{
  sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
  },
  thread,
  time::Duration,
};

use alsa::{
  Direction, ValueOr,
  pcm::{Access, Format, HwParams, PCM},
};
use tokio::sync::mpsc;
use uuid::Uuid;

use super::{CapturedFrame, MicConfig, MicError};

#[derive(Debug)]
pub(super) struct WorkerHandle {
  cancel: Arc<AtomicBool>,
}

impl WorkerHandle {
  pub fn start(stream_id: Uuid, config: MicConfig, frames: mpsc::Sender<CapturedFrame>) -> Result<Self, MicError> {
    let cancel = Arc::new(AtomicBool::new(false));
    let cancel_clone = cancel.clone();
    let pcm = open_pcm(&config).map_err(|e| MicError::Alsa(e.to_string()))?;
    pcm.start().map_err(|e| MicError::Alsa(e.to_string()))?;

    thread::Builder::new()
      .name(format!("mic-capture-{stream_id}"))
      .spawn(move || {
        if let Err(err) = run(pcm, stream_id, config, frames, cancel_clone) {
          tracing::warn!("mic capture worker exited with error: {err}");
        }
      })
      .map_err(|e| MicError::Alsa(format!("spawn worker thread: {e}")))?;

    Ok(Self { cancel })
  }

  pub fn stop(self) {
    self.cancel.store(true, Ordering::Release);
  }
}

fn open_pcm(config: &MicConfig) -> alsa::Result<PCM> {
  let pcm = PCM::new(&config.device, Direction::Capture, false)?;
  {
    let hwp = HwParams::any(&pcm)?;
    hwp.set_channels(config.format.channels.into())?;
    hwp.set_rate(config.format.sample_rate_hz, ValueOr::Nearest)?;
    hwp.set_format(Format::s16())?;
    hwp.set_access(Access::RWInterleaved)?;
    pcm.hw_params(&hwp)?;
  }
  Ok(pcm)
}

fn run(
  pcm: PCM,
  stream_id: Uuid,
  config: MicConfig,
  frames: mpsc::Sender<CapturedFrame>,
  cancel: Arc<AtomicBool>,
) -> alsa::Result<()> {
  let io = pcm.io_i16()?;
  let frame_samples = (config.format.frame_samples as usize) * (config.format.channels as usize);
  let mut buf = vec![0i16; frame_samples];
  let mut seq: u32 = 0;

  loop {
    if cancel.load(Ordering::Acquire) {
      let _ = pcm.drop();
      return Ok(());
    }
    match io.readi(&mut buf) {
      Ok(read) if read == config.format.frame_samples as usize => {
        let bytes = pcm_to_bytes(&buf[..read * config.format.channels as usize]);
        let frame = CapturedFrame {
          stream_id,
          seq,
          pcm: bytes,
        };
        if frames.blocking_send(frame).is_err() {
          let _ = pcm.drop();
          return Ok(());
        }
        seq = seq.wrapping_add(1);
      }
      Ok(_) => continue,
      Err(err) => {
        tracing::trace!("mic capture read error: {err}; recovering");
        if pcm.recover(err.errno(), true).is_err() {
          thread::sleep(Duration::from_millis(20));
        }
      }
    }
  }
}

fn pcm_to_bytes(samples: &[i16]) -> Vec<u8> {
  let mut bytes = Vec::with_capacity(samples.len() * 2);
  for sample in samples {
    bytes.extend_from_slice(&sample.to_le_bytes());
  }
  bytes
}
