use std::{
  sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
  },
  thread,
  time::Duration,
};

use alsa::{
  Direction, ValueOr,
  pcm::{Access, Format, HwParams, PCM},
};
use bridgething_dsp::{
  geometry::CHANNELS,
  pipeline::{Beamformer, Config as DspConfig, to_pcm16},
};
use bytes::{Bytes, BytesMut};
use tokio::sync::mpsc;

use crate::status::{SAMPLE_RATE_HZ, Telemetry, dbfs};

const FRAMES_PER_READ: usize = 1024;
const BUFFER_FRAMES: usize = 8192;
const TELEMETRY_EVERY: usize = 4;

#[derive(Debug)]
pub struct Chunk {
  pub raw: Bytes,
  pub beam: Bytes,
  pub front: Option<FrontEnd>,
}

#[derive(Debug, Clone, Copy)]
pub struct FrontEnd {
  pub channel_dbfs: [f32; CHANNELS],
  pub beam_dbfs: f32,
  pub bearing_deg: Option<f64>,
  pub steering_deg: f64,
  pub point_like: bool,
  pub adopted_bins: usize,
  pub noise_measured: bool,
}

impl FrontEnd {
  pub fn apply(&self, telemetry: &mut Telemetry) {
    telemetry.channel_dbfs = self.channel_dbfs;
    telemetry.beam_dbfs = self.beam_dbfs;
    telemetry.bearing_deg = self.bearing_deg;
    telemetry.steering_deg = self.steering_deg;
    telemetry.point_like = self.point_like;
    telemetry.adopted_bins = self.adopted_bins;
    telemetry.noise_measured = self.noise_measured;
  }
}

#[derive(Debug, thiserror::Error)]
#[error("alsa: {0}")]
pub struct CaptureError(String);

#[derive(Debug, Clone)]
pub struct CaptureHandle {
  mark_target: Arc<AtomicBool>,
  overruns: Arc<AtomicU64>,
}

impl CaptureHandle {
  pub fn mark_target(&self) {
    self.mark_target.store(true, Ordering::Release);
  }

  pub fn overruns(&self) -> u64 {
    self.overruns.load(Ordering::Relaxed)
  }
}

pub fn start(
  device: &str,
  dsp: DspConfig,
  chunks: mpsc::Sender<Chunk>,
  wake: mpsc::Sender<Bytes>,
) -> Result<CaptureHandle, CaptureError> {
  let pcm = open_pcm(device).map_err(|err| CaptureError(err.to_string()))?;
  pcm.start().map_err(|err| CaptureError(err.to_string()))?;

  let handle = CaptureHandle {
    mark_target: Arc::new(AtomicBool::new(false)),
    overruns: Arc::new(AtomicU64::new(0)),
  };
  let worker = handle.clone();
  thread::Builder::new()
    .name("mic-capture".into())
    .spawn(move || {
      if let Err(err) = run(pcm, dsp, chunks, wake, worker) {
        tracing::error!("capture thread exited: {err}");
      }
    })
    .map_err(|err| CaptureError(format!("spawn capture thread: {err}")))?;
  Ok(handle)
}

fn open_pcm(device: &str) -> alsa::Result<PCM> {
  let pcm = PCM::new(device, Direction::Capture, false)?;
  {
    let hwp = HwParams::any(&pcm)?;
    hwp.set_channels(CHANNELS as u32)?;
    hwp.set_rate(SAMPLE_RATE_HZ, ValueOr::Nearest)?;
    hwp.set_format(Format::s32())?;
    hwp.set_access(Access::RWInterleaved)?;
    hwp.set_buffer_size_near(BUFFER_FRAMES as alsa::pcm::Frames)?;
    hwp.set_period_size_near(FRAMES_PER_READ as alsa::pcm::Frames, ValueOr::Nearest)?;
    pcm.hw_params(&hwp)?;
  }
  Ok(pcm)
}

fn run(
  pcm: PCM,
  dsp: DspConfig,
  chunks: mpsc::Sender<Chunk>,
  wake: mpsc::Sender<Bytes>,
  handle: CaptureHandle,
) -> alsa::Result<()> {
  let io = pcm.io_i32()?;
  let mut beamformer = Beamformer::new(dsp);
  let mut samples = vec![0i32; FRAMES_PER_READ * CHANNELS];
  let mut mono: Vec<f32> = Vec::with_capacity(FRAMES_PER_READ * 2);
  let mut beam = BytesMut::with_capacity(FRAMES_PER_READ * 2);
  let mut since_telemetry = 0usize;

  loop {
    if chunks.is_closed() {
      let _ = pcm.drop();
      return Ok(());
    }
    if handle.mark_target.swap(false, Ordering::AcqRel) {
      beamformer.mark_target();
      tracing::debug!(bearing = ?beamformer.bearing_deg(), "beamformer marked a target");
    }

    let read = match io.readi(&mut samples) {
      Ok(0) => continue,
      Ok(read) => read,
      Err(err) => {
        tracing::trace!("capture read error: {err}; recovering");
        if pcm.recover(err.errno(), true).is_err() {
          thread::sleep(Duration::from_millis(20));
        }
        continue;
      }
    };

    let interleaved = &samples[..read * CHANNELS];
    mono.clear();
    beamformer.process(interleaved, &mut mono);
    beam.clear();
    to_pcm16(&mono, &mut beam);
    let beam = beam.split().freeze();

    if wake.try_send(beam.clone()).is_err() {
      tracing::trace!("wake word is behind; dropping a chunk");
    }

    since_telemetry += 1;
    let front = (since_telemetry >= TELEMETRY_EVERY).then(|| {
      since_telemetry = 0;
      let (adopted_bins, noise_measured) = beamformer.adoption();
      FrontEnd {
        channel_dbfs: channel_levels(interleaved, read),
        beam_dbfs: level(&mono),
        bearing_deg: beamformer.bearing_deg(),
        steering_deg: beamformer.config().steering_deg,
        point_like: beamformer.point_like(),
        adopted_bins,
        noise_measured,
      }
    });

    let chunk = Chunk {
      raw: raw_bytes(interleaved),
      beam,
      front,
    };
    if chunks.try_send(chunk).is_err() {
      handle.overruns.fetch_add(1, Ordering::Relaxed);
    }
  }
}

fn raw_bytes(interleaved: &[i32]) -> Bytes {
  let mut out = BytesMut::with_capacity(interleaved.len() * 4);
  for sample in interleaved {
    out.extend_from_slice(&sample.to_le_bytes());
  }
  out.freeze()
}

fn channel_levels(interleaved: &[i32], frames: usize) -> [f32; CHANNELS] {
  let mut sums = [0f64; CHANNELS];
  for frame in interleaved.chunks_exact(CHANNELS) {
    for (channel, &sample) in frame.iter().enumerate() {
      let scaled = sample as f64 / 2_147_483_648.0;
      sums[channel] += scaled * scaled;
    }
  }
  std::array::from_fn(|channel| dbfs(sums[channel], frames))
}

fn level(mono: &[f32]) -> f32 {
  dbfs(mono.iter().map(|&s| (s as f64) * (s as f64)).sum(), mono.len())
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::status::SILENCE_DBFS;

  #[test]
  fn raw_bytes_stay_little_endian_and_interleaved() {
    let bytes = raw_bytes(&[1i32, -1i32]);
    assert_eq!(&bytes[..4], &1i32.to_le_bytes());
    assert_eq!(&bytes[4..], &(-1i32).to_le_bytes());
  }

  #[test]
  fn a_dead_channel_shows_up_in_the_per_channel_levels() {
    let loud = i32::MAX / 2;
    let frames: Vec<i32> = (0..64).flat_map(|_| [loud, loud, 0, loud]).collect();
    let levels = channel_levels(&frames, 64);
    assert_eq!(levels[2], SILENCE_DBFS);
    assert!(levels[0] > -10.0);
  }

  #[test]
  fn silence_reads_as_the_floor_rather_than_negative_infinity() {
    assert_eq!(level(&[0.0; 32]), SILENCE_DBFS);
  }

  #[test]
  fn front_end_telemetry_lands_on_the_status_struct_intact() {
    let front = FrontEnd {
      channel_dbfs: [-20.0, -21.0, -22.0, -23.0],
      beam_dbfs: -18.0,
      bearing_deg: Some(41.0),
      steering_deg: 35.0,
      point_like: true,
      adopted_bins: 47,
      noise_measured: true,
    };
    let mut telemetry = Telemetry::default();
    front.apply(&mut telemetry);
    assert_eq!(telemetry.bearing_deg, Some(41.0));
    assert_eq!(telemetry.adopted_bins, 47);
    assert!(telemetry.point_like);
  }
}
