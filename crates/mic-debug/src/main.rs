mod capture;
mod drive;
mod input;
mod recorder;
mod session;
mod status;
mod usb;
mod wake;
mod web;

use std::net::SocketAddr;

use bridgething_dsp::pipeline::Config as DspConfig;
use tokio::sync::mpsc;
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

use crate::{input::Command, recorder::Recorder, status::Shared};

pub const TAGS: &[&str] = &[
  "untagged",
  "parked",
  "city",
  "highway",
  "windows down",
  "music loud",
  "hvac high",
  "passengers",
];

pub const WAKEWORD_THRESHOLD: f32 = 0.35;
const ALSA_DEVICE: &str = "hw:0,0";
const HTTP_PORT: u16 = 8099;
const CHUNK_BACKLOG: usize = 128;

fn main() {
  tracing_subscriber::registry()
    .with(EnvFilter::try_from_env("MIC_DEBUG_LOG").unwrap_or_else(|_| EnvFilter::new("info")))
    .with(tracing_subscriber::fmt::layer())
    .init();

  let shared = Shared::new();
  let (commands_tx, commands_rx) = mpsc::channel::<Command>(16);
  let (chunks_tx, chunks_rx) = mpsc::channel(CHUNK_BACKLOG);
  let (wake_tx, wake_rx) = mpsc::channel(64);

  let recorder_shared = shared.clone();
  let recorder_thread = std::thread::Builder::new()
    .name("recorder".into())
    .spawn(move || {
      let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("recorder runtime");
      runtime.block_on(async move {
        let dsp = dsp_config();
        let (wake_pcm, loaded) = match wake::spawn(&wake::model_paths(), WAKEWORD_THRESHOLD, wake_tx) {
          Some((pcm, loaded)) => {
            recorder_shared.update(|status| status.wakeword_loaded = true);
            (Some(pcm), Some(loaded))
          }
          None => {
            recorder_shared.alert("no wake word model on this image - audio still records, scores read zero");
            (None, None)
          }
        };

        let (dead_pcm, _dead_rx) = mpsc::channel(1);
        let capture = match capture::start(ALSA_DEVICE, dsp, chunks_tx, wake_pcm.unwrap_or(dead_pcm)) {
          Ok(handle) => {
            recorder_shared.update(|status| status.mic_open = true);
            handle
          }
          Err(err) => {
            recorder_shared.set_stage(status::Stage::Faulted {
              session: None,
              what: format!("microphone unavailable: {err}"),
            });
            return;
          }
        };

        let meta = recorder::session_meta(
          loaded.as_ref().map(|l| l.model.display().to_string()),
          WAKEWORD_THRESHOLD,
          dsp,
        );
        Recorder::new(recorder_shared, capture, chunks_rx, wake_rx, commands_rx, meta)
          .run()
          .await;
      });
    })
    .expect("spawn recorder thread");

  let runtime = tokio::runtime::Runtime::new().expect("main runtime");
  runtime.block_on(async move {
    let addr = SocketAddr::from(([0, 0, 0, 0], HTTP_PORT));
    let listener = match web::bind(addr).await {
      Ok(listener) => listener,
      Err(err) => {
        tracing::error!("could not bind the debug ui on {addr}: {err}");
        return;
      }
    };

    set_initial_usb_role(&shared);
    tokio::spawn(input::listen(commands_tx.clone(), shared.clone()));
    if let Err(err) = web::serve(listener, shared, commands_tx).await {
      tracing::error!("debug ui server exited: {err}");
    }
  });

  let _ = recorder_thread.join();
}

fn dsp_config() -> DspConfig {
  DspConfig {
    adaptation: Some(bridgething_dsp::scene::Config::default()),
    ..DspConfig::default()
  }
}

fn set_initial_usb_role(shared: &Shared) {
  if usb::stay_device() {
    shared.alert("boot role is pinned to device - staying in gadget mode, so no drive will enumerate");
  } else if let Err(err) = usb::set_role("host") {
    shared.alert(format!("could not put the usb port in host mode: {err}"));
  }
  let role = usb::role();
  shared.update(|status| status.usb_role = role);
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn the_default_tag_claims_nothing_about_the_drive() {
    assert_eq!(TAGS[0], "untagged");
  }

  #[test]
  fn the_threshold_is_the_one_the_device_ships() {
    assert_eq!(WAKEWORD_THRESHOLD, 0.35);
  }

  #[test]
  fn scene_adaptation_is_on_because_the_daemon_runs_it_on() {
    assert!(dsp_config().adaptation.is_some());
  }
}
