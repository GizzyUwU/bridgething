use std::sync::{Arc, RwLock};

use bridgething_gateway::{AudioHandler, OutboundLink, OutboundLinkExt};
use libbridgething::{
  AudioError,
  gateway::{
    AudioErrorReply, Earcon, GatewayToBridgeAudioMsgEvent, SetMute, SetVolume, Tts, TtsCancel, TtsEnded, TtsStarted,
    VolumeChanged,
  },
  wire::WireError,
};

use crate::{
  backend::{AudioBackend, EarconSink, SpeakEvent, SpeakSink},
  dispatch::{Serial, tell},
};

#[async_trait::async_trait]
pub trait VolumeAuthority: Send + Sync {
  async fn owns_volume(&self) -> bool;
  async fn volume_up(&self) -> Result<f32, String>;
  async fn volume_down(&self) -> Result<f32, String>;
  async fn set_volume(&self, level: f32) -> Result<f32, String>;
}

pub struct AudioDispatcher {
  backend: Arc<dyn AudioBackend>,
  link: Arc<dyn OutboundLink>,
  authority: RwLock<Option<Arc<dyn VolumeAuthority>>>,
  speech: Serial,
  earcons: Serial,
}

impl AudioDispatcher {
  pub fn new(backend: Arc<dyn AudioBackend>, link: Arc<dyn OutboundLink>) -> Self {
    Self {
      backend,
      link,
      authority: RwLock::new(None),
      speech: Serial::spawn(),
      earcons: Serial::spawn(),
    }
  }

  pub fn set_volume_authority(&self, authority: Option<Arc<dyn VolumeAuthority>>) {
    *self.authority.write().unwrap() = authority;
  }

  pub async fn stop(&self) {
    tell(&self.backend, |backend| backend.cancel_all()).await;
  }

  async fn volume_owner(&self) -> Option<Arc<dyn VolumeAuthority>> {
    let installed = self.authority.read().unwrap().clone();
    match installed {
      Some(authority) if authority.owns_volume().await => Some(authority),
      _ => None,
    }
  }

  async fn moved(&self, verb: &str, outcome: Result<f32, String>) {
    match outcome {
      Ok(level) => {
        let _ = self
          .link
          .event(GatewayToBridgeAudioMsgEvent::VolumeChanged(VolumeChanged {
            level,
            muted: false,
          }))
          .await;
      }
      Err(reason) => {
        self
          .report(AudioError::ActionRejected {
            reason: format!("{verb}: {reason}"),
          })
          .await;
      }
    }
  }

  async fn report(&self, error: AudioError) {
    let _ = self
      .link
      .event(GatewayToBridgeAudioMsgEvent::ErrorEvent(AudioErrorReply { error }))
      .await;
  }
}

impl AudioHandler for AudioDispatcher {
  async fn volume_up(&self) -> Result<(), WireError> {
    match self.volume_owner().await {
      Some(authority) => self.moved("volumeUp", authority.volume_up().await).await,
      None => tell(&self.backend, |backend| backend.volume_up()).await,
    }
    Ok(())
  }

  async fn volume_down(&self) -> Result<(), WireError> {
    match self.volume_owner().await {
      Some(authority) => self.moved("volumeDown", authority.volume_down().await).await,
      None => tell(&self.backend, |backend| backend.volume_down()).await,
    }
    Ok(())
  }

  async fn set_volume(&self, payload: SetVolume) -> Result<(), WireError> {
    let level = payload.level;
    match self.volume_owner().await {
      Some(authority) => self.moved("setVolume", authority.set_volume(level).await).await,
      None => tell(&self.backend, move |backend| backend.set_volume(level)).await,
    }
    Ok(())
  }

  async fn mute_toggle(&self) -> Result<(), WireError> {
    if self.volume_owner().await.is_none() {
      tell(&self.backend, |backend| backend.mute_toggle()).await;
    }
    Ok(())
  }

  async fn set_mute(&self, payload: SetMute) -> Result<(), WireError> {
    if self.volume_owner().await.is_none() {
      let muted = payload.muted;
      tell(&self.backend, move |backend| backend.set_mute(muted)).await;
    }
    Ok(())
  }

  async fn tts(&self, payload: Tts) -> Result<(), WireError> {
    self
      .speech
      .push(speak(self.backend.clone(), self.link.clone(), payload));
    Ok(())
  }

  async fn tts_cancel(&self, payload: TtsCancel) -> Result<(), WireError> {
    let id = payload.id.to_string();
    tell(&self.backend, move |backend| backend.cancel(id)).await;
    Ok(())
  }

  async fn tts_cancel_all(&self) -> Result<(), WireError> {
    tell(&self.backend, |backend| backend.cancel_all()).await;
    Ok(())
  }

  async fn earcon(&self, payload: Earcon) -> Result<(), WireError> {
    self
      .earcons
      .push(play(self.backend.clone(), self.link.clone(), payload.name));
    Ok(())
  }
}

async fn speak(backend: Arc<dyn AudioBackend>, link: Arc<dyn OutboundLink>, payload: Tts) {
  let Tts { id, text, voice } = payload;
  let (sink, mut events) = SpeakSink::channel();
  let spoken = id.to_string();
  tell(&backend, move |backend| backend.speak(spoken, text, voice, sink)).await;

  while let Some(event) = events.recv().await {
    match event {
      SpeakEvent::Started => {
        let _ = link
          .event(GatewayToBridgeAudioMsgEvent::TtsStarted(TtsStarted { id }))
          .await;
      }
      SpeakEvent::Finished { ok } => {
        let _ = link
          .event(GatewayToBridgeAudioMsgEvent::TtsEnded(TtsEnded { id, completed: ok }))
          .await;
        return;
      }
    }
  }

  let _ = link
    .event(GatewayToBridgeAudioMsgEvent::TtsEnded(TtsEnded {
      id,
      completed: false,
    }))
    .await;
}

async fn play(backend: Arc<dyn AudioBackend>, link: Arc<dyn OutboundLink>, name: String) {
  let (sink, played) = EarconSink::channel();
  tell(&backend, move |backend| backend.play_earcon(name, sink)).await;
  if !matches!(played.await, Ok(true)) {
    let _ = link
      .event(GatewayToBridgeAudioMsgEvent::ErrorEvent(AudioErrorReply {
        error: AudioError::Unavailable { verb: "earcon".into() },
      }))
      .await;
  }
}
