//! ALS (ambient light sensor) driver + backlight policy. Owns the
//! TMD2772 IIO node and the pwm-backlight sysfs entry; reads the
//! photodiode raw count, applies the same EMA + log10 + hysteresis
//! curve the standalone bridgething-als C daemon uses, and either
//! drives `/sys/class/backlight/backlight/brightness` directly (Auto)
//! or holds a webapp-set level (Manual).
//!
//! Wire surface: `BridgeToClientHardwareMsg::AmbientLightUpdate` fires
//! on raw-sample changes; `BrightnessChanged` fires on mode/level/
//! effective-level transitions; `HardwareStateGet` returns a snapshot.

use std::{
  path::{Path, PathBuf},
  sync::Arc,
  time::Duration,
};

use libbridgething::{
  BrightnessMode, BrightnessState, HardwareError, HardwareState,
  client::{AmbientLightUpdate, BridgeToClientHardwareMsg, HardwareStateReply},
  wire::MsgMeta,
};
use tokio::{
  sync::{RwLock, mpsc, oneshot},
  task::JoinHandle,
};

use crate::net::WireEventBus;

const ALS_PATH: &str = "/sys/bus/iio/devices/iio:device0/in_intensity0_raw";
const BACKLIGHT_DIR: &str = "/sys/class/backlight/backlight";

#[derive(Debug, Clone)]
pub struct AlsConfig {
  pub poll_interval: Duration,
  pub ema_alpha: f64,
  pub raw_at_max: f64,
  pub min_brightness: u32,
  pub hysteresis: u32,
  pub zero_streak_limit: u32,
  pub als_path: PathBuf,
  pub backlight_dir: PathBuf,
}

impl Default for AlsConfig {
  fn default() -> Self {
    Self {
      poll_interval: Duration::from_millis(200),
      ema_alpha: 0.20,
      raw_at_max: 500.0,
      min_brightness: 64,
      hysteresis: 2,
      zero_streak_limit: 5,
      als_path: PathBuf::from(ALS_PATH),
      backlight_dir: PathBuf::from(BACKLIGHT_DIR),
    }
  }
}

#[derive(Debug, thiserror::Error)]
pub enum AlsError {
  #[error("backlight sysfs path does not exist: {0}")]
  BacklightAbsent(PathBuf),
  #[error("io: {0}")]
  Io(#[from] std::io::Error),
  #[error("manager loop has exited")]
  Closed,
}

#[derive(Debug)]
struct Inner {
  config: AlsConfig,
  mode: BrightnessMode,
  manual_level: f32,
  effective_level: f32,
  ambient_raw: u32,
  smoothed: f64,
  zero_streak: u32,
  max_brightness: u32,
}

impl Inner {
  fn new(config: AlsConfig, max_brightness: u32) -> Self {
    Self {
      config,
      mode: BrightnessMode::Auto,
      manual_level: 1.0,
      effective_level: 1.0,
      ambient_raw: 0,
      smoothed: -1.0,
      zero_streak: 0,
      max_brightness,
    }
  }

  fn snapshot(&self) -> HardwareState {
    HardwareState {
      brightness: BrightnessState {
        mode: self.mode,
        level: self.manual_level,
        effective_level: self.effective_level,
      },
      ambient_light: self.ambient_raw,
    }
  }

  fn brightness_for_ambient(&self) -> f32 {
    if self.smoothed < 0.0 || self.max_brightness == 0 {
      return self.effective_level;
    }
    let log_max = (1.0 + self.config.raw_at_max).log10();
    let mut ratio = (1.0 + self.smoothed).log10() / log_max;
    ratio = ratio.clamp(0.0, 1.0);
    let min = self.config.min_brightness as f32 / self.max_brightness as f32;
    min + (1.0 - min) * ratio as f32
  }

  fn level_to_ticks(&self, level: f32) -> u32 {
    let level = level.clamp(0.0, 1.0);
    (level * self.max_brightness as f32 + 0.5) as u32
  }
}

#[derive(Debug)]
enum Cmd {
  SetMode(BrightnessMode, oneshot::Sender<()>),
  SetLevel(f32, oneshot::Sender<Result<(), HardwareError>>),
}

#[derive(Debug, Clone)]
pub struct AlsManager {
  inner: Arc<RwLock<Inner>>,
  tx: mpsc::Sender<Cmd>,
}

impl AlsManager {
  pub async fn init(bus: WireEventBus, config: AlsConfig) -> Result<AlsManagerInit, AlsError> {
    let max_brightness = read_max_brightness(&config.backlight_dir).await.unwrap_or_else(|_| {
      tracing::warn!(
        "als: unable to read {}/max_brightness; deferring backlight policy until paths exist",
        config.backlight_dir.display(),
      );
      0
    });
    if max_brightness > 0 {
      tracing::info!(
        "als: initialized (max_brightness={max_brightness}, min={}, raw_at_max={}, alpha={}, hysteresis={})",
        config.min_brightness,
        config.raw_at_max,
        config.ema_alpha,
        config.hysteresis,
      );
    }

    let inner = Arc::new(RwLock::new(Inner::new(config, max_brightness)));
    let (tx, rx) = mpsc::channel(16);
    Ok(AlsManagerInit {
      manager: Self {
        inner: inner.clone(),
        tx,
      },
      rx,
      inner,
      bus,
    })
  }

  pub async fn snapshot(&self) -> HardwareState {
    self.inner.read().await.snapshot()
  }

  pub async fn snapshot_reply(&self) -> HardwareStateReply {
    HardwareStateReply {
      state: self.snapshot().await,
    }
  }

  pub async fn set_mode(&self, mode: BrightnessMode) -> Result<(), AlsError> {
    let (reply_tx, reply_rx) = oneshot::channel();
    self
      .tx
      .send(Cmd::SetMode(mode, reply_tx))
      .await
      .map_err(|_| AlsError::Closed)?;
    reply_rx.await.map_err(|_| AlsError::Closed)
  }

  pub async fn set_level(&self, level: f32) -> Result<Result<(), HardwareError>, AlsError> {
    let (reply_tx, reply_rx) = oneshot::channel();
    self
      .tx
      .send(Cmd::SetLevel(level, reply_tx))
      .await
      .map_err(|_| AlsError::Closed)?;
    reply_rx.await.map_err(|_| AlsError::Closed)
  }
}

pub struct AlsManagerInit {
  pub manager: AlsManager,
  rx: mpsc::Receiver<Cmd>,
  inner: Arc<RwLock<Inner>>,
  bus: WireEventBus,
}

impl AlsManagerInit {
  pub fn spawn(self) -> (AlsManager, JoinHandle<()>) {
    let manager = self.manager.clone();
    let handle = tokio::spawn(run_loop(self.rx, self.inner, self.bus));
    (manager, handle)
  }
}

async fn run_loop(mut rx: mpsc::Receiver<Cmd>, inner: Arc<RwLock<Inner>>, bus: WireEventBus) {
  let mut interval = {
    let guard = inner.read().await;
    tokio::time::interval(guard.config.poll_interval)
  };
  interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

  loop {
    tokio::select! {
      cmd = rx.recv() => {
        let Some(cmd) = cmd else { break; };
        handle_cmd(cmd, &inner, &bus).await;
      }
      _ = interval.tick() => {
        if let Err(err) = poll_once(&inner, &bus).await {
          tracing::trace!("als poll failed: {err}");
        }
      }
    }
  }
  tracing::debug!("als manager loop exiting");
}

async fn handle_cmd(cmd: Cmd, inner: &Arc<RwLock<Inner>>, bus: &WireEventBus) {
  match cmd {
    Cmd::SetMode(mode, reply) => {
      let brightness = {
        let mut guard = inner.write().await;
        if guard.mode == mode {
          let _ = reply.send(());
          return;
        }
        guard.mode = mode;
        if mode == BrightnessMode::Manual {
          guard.effective_level = guard.manual_level;
        } else {
          guard.effective_level = guard.brightness_for_ambient();
        }
        let ticks = guard.level_to_ticks(guard.effective_level);
        let dir = guard.config.backlight_dir.clone();
        let snapshot = guard.snapshot().brightness;
        drop(guard);
        write_brightness(&dir, ticks).await.ok();
        snapshot
      };
      let _ = reply.send(());
      broadcast(bus, BridgeToClientHardwareMsg::BrightnessChanged(brightness)).await;
    }
    Cmd::SetLevel(level, reply) => {
      if !(0.0..=1.0).contains(&level) {
        let _ = reply.send(Err(HardwareError::LevelOutOfRange));
        return;
      }
      let (mismatch, brightness) = {
        let mut guard = inner.write().await;
        guard.manual_level = level;
        if guard.mode != BrightnessMode::Manual {
          let snapshot = guard.snapshot().brightness;
          (true, snapshot)
        } else {
          guard.effective_level = level;
          let ticks = guard.level_to_ticks(level);
          let dir = guard.config.backlight_dir.clone();
          let snapshot = guard.snapshot().brightness;
          drop(guard);
          write_brightness(&dir, ticks).await.ok();
          (false, snapshot)
        }
      };
      let outcome = if mismatch {
        Err(HardwareError::ModeMismatch)
      } else {
        Ok(())
      };
      let _ = reply.send(outcome);
      broadcast(bus, BridgeToClientHardwareMsg::BrightnessChanged(brightness)).await;
    }
  }
}

async fn poll_once(inner: &Arc<RwLock<Inner>>, bus: &WireEventBus) -> Result<(), AlsError> {
  let (mode, max_brightness, als_path) = {
    let guard = inner.read().await;
    (guard.mode, guard.max_brightness, guard.config.als_path.clone())
  };
  let sample = match read_raw(&als_path).await {
    Ok(v) => v,
    Err(_) => return Ok(()),
  };
  if max_brightness == 0 {
    let max = read_max_brightness(&inner.read().await.config.backlight_dir).await?;
    inner.write().await.max_brightness = max;
  }

  let (ambient_changed, brightness_changed, ticks_to_write, dir, brightness_state, ambient_event) = {
    let mut guard = inner.write().await;

    if sample == 0 && guard.smoothed > 1.0 && guard.zero_streak < guard.config.zero_streak_limit {
      guard.zero_streak += 1;
      return Ok(());
    }
    guard.zero_streak = 0;

    if guard.smoothed < 0.0 {
      guard.smoothed = sample as f64;
    } else {
      guard.smoothed = guard.config.ema_alpha * sample as f64 + (1.0 - guard.config.ema_alpha) * guard.smoothed;
    }

    let prev_raw = guard.ambient_raw;
    guard.ambient_raw = sample;
    let ambient_changed = sample != prev_raw;

    let prev_effective = guard.effective_level;
    let mut brightness_changed = false;
    let mut ticks_to_write: Option<u32> = None;
    if mode == BrightnessMode::Auto {
      let new_effective = guard.brightness_for_ambient();
      let new_ticks = guard.level_to_ticks(new_effective);
      let prev_ticks = guard.level_to_ticks(prev_effective);
      if new_ticks.abs_diff(prev_ticks) >= guard.config.hysteresis {
        guard.effective_level = new_effective;
        ticks_to_write = Some(new_ticks);
        brightness_changed = true;
      }
    }
    let dir = guard.config.backlight_dir.clone();
    let snapshot = guard.snapshot();
    let ambient_event = AmbientLightUpdate { brightness: sample };
    (
      ambient_changed,
      brightness_changed,
      ticks_to_write,
      dir,
      snapshot.brightness,
      ambient_event,
    )
  };

  if let Some(ticks) = ticks_to_write {
    write_brightness(&dir, ticks).await.ok();
  }
  if ambient_changed {
    broadcast(bus, BridgeToClientHardwareMsg::AmbientLightUpdate(ambient_event)).await;
  }
  if brightness_changed {
    broadcast(bus, BridgeToClientHardwareMsg::BrightnessChanged(brightness_state)).await;
  }
  Ok(())
}

async fn read_raw(path: &Path) -> Result<u32, AlsError> {
  let bytes = tokio::fs::read(path).await?;
  Ok(parse_uint(&bytes))
}

async fn read_max_brightness(dir: &Path) -> Result<u32, AlsError> {
  let path = dir.join("max_brightness");
  if !tokio::fs::try_exists(&path).await? {
    return Err(AlsError::BacklightAbsent(dir.to_path_buf()));
  }
  let bytes = tokio::fs::read(&path).await?;
  Ok(parse_uint(&bytes))
}

async fn write_brightness(dir: &Path, ticks: u32) -> Result<(), AlsError> {
  let path = dir.join("brightness");
  tokio::fs::write(path, format!("{ticks}\n")).await?;
  Ok(())
}

fn parse_uint(bytes: &[u8]) -> u32 {
  let s = std::str::from_utf8(bytes).unwrap_or("0").trim();
  s.parse().unwrap_or(0)
}

async fn broadcast(bus: &WireEventBus, event: BridgeToClientHardwareMsg) {
  if let Err(errors) = bus.broadcast(event, MsgMeta::Event).await {
    tracing::trace!("als broadcast had {} ws error(s)", errors.len());
  }
}
