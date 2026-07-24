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
use bytes::{BufMut, BytesMut};
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
  let channels = config.format.channels as usize;
  let frame_samples = (config.format.frame_samples as usize) * channels;
  let frame_byte_len = frame_samples * 2;
  let mut samples = vec![0i16; frame_samples];
  let mut bytes_buf = BytesMut::with_capacity(frame_byte_len);
  let mut seq: u32 = 0;

  loop {
    if cancel.load(Ordering::Acquire) {
      let _ = pcm.drop();
      return Ok(());
    }
    bytes_buf.reserve(frame_byte_len);
    match io.readi(&mut samples) {
      Ok(read) if read == config.format.frame_samples as usize => {
        let take = read * channels;
        for sample in &samples[..take] {
          bytes_buf.put_i16_le(*sample);
        }
        let frame = CapturedFrame {
          stream_id,
          seq,
          pcm: bytes_buf.split().freeze(),
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
