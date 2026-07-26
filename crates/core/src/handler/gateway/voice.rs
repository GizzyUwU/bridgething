use libbridgething::{
  BrightnessMode, ItemKind, ItemRef, NluBrightnessMode, NluDirection, NluPhoneAction, NluPlaybackSpeed, NluRepeatMode,
  NluResolvedIntent, NluSlots, NluSystemAction, RepeatMode, VoiceDispatchErrorCode, VoiceDispatchTarget,
  client::{BridgeToClientVoiceMsgEvent, VoiceDisplayIntent, VoiceIntent},
  gateway::{
    self, BridgeToGatewayAudioMsgCommand, BridgeToGatewayLibraryMsgCommand, BridgeToGatewayPlayerMsgCommand,
    BridgeToGatewayVoiceMsg, GatewayToBridgeVoiceMsgCommandDispatch, VoiceCloseReason, VoiceDispatch,
    VoiceDispatchFailed, VoiceDispatched, VoiceMicOpen,
  },
};
use uuid::Uuid;

use super::{HandlerResult, MsgHandle, webapp::navigate_url_for_active};
use crate::{chrome::ChromeCommand, state::TelephonyManager, systemd::power};

const BRIGHTNESS_STEP: f32 = 0.15;

type Routed = Result<(VoiceDispatchTarget, Option<Uuid>), (VoiceDispatchErrorCode, String)>;

fn bad(msg: impl Into<String>) -> (VoiceDispatchErrorCode, String) {
  (VoiceDispatchErrorCode::BadSlots, msg.into())
}

pub struct VoiceHandler {
  handle: MsgHandle,
}

impl VoiceHandler {
  pub fn new(handle: MsgHandle) -> Self {
    Self { handle }
  }
}

impl GatewayToBridgeVoiceMsgCommandDispatch for VoiceHandler {
  type Output = HandlerResult;

  async fn mic_open(&self, _params: VoiceMicOpen) -> HandlerResult {
    match self.handle.state.mic.push_to_talk().await {
      Ok(stream_id) => tracing::debug!("({:?}) gateway opened mic -> stream {stream_id}", &self.handle.address),
      Err(err) => tracing::warn!("({:?}) gateway mic open failed: {err}", &self.handle.address),
    }
    Ok(())
  }

  async fn mic_close(&self) -> HandlerResult {
    if let Err(err) = self.handle.state.mic.stop_with(VoiceCloseReason::Cancelled).await {
      tracing::warn!("({:?}) gateway mic close failed: {err}", &self.handle.address);
    }
    Ok(())
  }

  async fn dispatch(&self, params: VoiceDispatch) -> HandlerResult {
    let VoiceDispatch { resolved } = params;
    tracing::info!(
      "({:?}) voice dispatch: intent={} transcript={:?}",
      &self.handle.address,
      resolved.intent,
      resolved.transcript,
    );

    match self.route(&resolved).await {
      Ok((target, webapp_id)) => {
        tracing::debug!("({:?}) voice dispatch -> {target:?}", &self.handle.address);
        self
          .handle
          .send_info(BridgeToGatewayVoiceMsg::Dispatched(VoiceDispatched {
            target,
            intent: resolved.intent.clone(),
            webapp_id: webapp_id.map(|id| id.to_string()),
          }))
          .await;
      }
      Err((code, msg)) => {
        tracing::debug!("({:?}) voice dispatch refused: {code:?}: {msg}", &self.handle.address);
        self
          .handle
          .send_info(BridgeToGatewayVoiceMsg::DispatchFailed(VoiceDispatchFailed {
            code,
            intent: resolved.intent.clone(),
            msg,
          }))
          .await;
      }
    }
    Ok(())
  }
}

impl VoiceHandler {
  async fn route(&self, resolved: &NluResolvedIntent) -> Routed {
    let slots = &resolved.slots;
    match resolved.intent.as_str() {
      "PLAY" if has_catalog_slots(slots) => self.play_catalog(slots).await,
      "PLAY" => {
        self.handle.transport.play().await;
        Ok((VoiceDispatchTarget::Playback, None))
      }
      "PAUSE" => {
        self.handle.transport.pause().await;
        Ok((VoiceDispatchTarget::Playback, None))
      }
      "NEXT" => {
        self.handle.transport.next().await;
        Ok((VoiceDispatchTarget::Playback, None))
      }
      "PREVIOUS" => {
        self.handle.transport.prev(true).await;
        Ok((VoiceDispatchTarget::Playback, None))
      }
      "SET_SHUFFLE" => {
        let enabled = slots.enabled.ok_or_else(|| bad("SET_SHUFFLE without `enabled`"))?;
        self.handle.transport.set_shuffle(enabled).await;
        Ok((VoiceDispatchTarget::Playback, None))
      }
      "SET_REPEAT" => {
        let mode = slots
          .repeat_mode
          .ok_or_else(|| bad("SET_REPEAT without `repeatMode`"))?;
        self.handle.transport.set_repeat(repeat_mode(mode)).await;
        Ok((VoiceDispatchTarget::Playback, None))
      }
      "SEEK_RELATIVE" => {
        let seconds = slots.seconds.ok_or_else(|| bad("SEEK_RELATIVE without `seconds`"))?;
        let current = self.handle.state.player.position_ms() as i64;
        let target = (current + seconds as i64 * 1000).max(0) as u32;
        self.handle.transport.seek_to(target).await;
        Ok((VoiceDispatchTarget::Playback, None))
      }
      "SET_PLAYBACK_SPEED" => {
        let speed = slots.speed.ok_or_else(|| bad("SET_PLAYBACK_SPEED without `speed`"))?;
        self
          .send_player(BridgeToGatewayPlayerMsgCommand::SetSpeed(gateway::SetSpeed {
            speed: playback_speed(speed),
          }))
          .await;
        Ok((VoiceDispatchTarget::Playback, None))
      }
      "SET_MUTE" => {
        let enabled = slots.enabled.ok_or_else(|| bad("SET_MUTE without `enabled`"))?;
        self
          .send_audio(BridgeToGatewayAudioMsgCommand::SetMute(gateway::SetMute {
            muted: enabled,
          }))
          .await;
        Ok((VoiceDispatchTarget::Playback, None))
      }
      "SET_VOLUME" => self.set_volume(slots).await,
      "PLAY_PRESET" => self.play_preset(slots).await,
      "SAVE_TO_PRESET" => self.save_to_preset(slots).await,
      "ADD_TO_QUEUE" => {
        let uri = slots.uri.clone().ok_or((
          VoiceDispatchErrorCode::PlaybackFailed,
          "no resolved uri to queue".into(),
        ))?;
        self
          .send_player(BridgeToGatewayPlayerMsgCommand::Queue(gateway::QueueUri {
            uri,
            position: libbridgething::QueuePosition::Append,
          }))
          .await;
        Ok((VoiceDispatchTarget::Playback, None))
      }
      "ADD_TO_COLLECTION" => self.favorite(slots, true).await,
      "THUMBS_UP" => self.favorite_current().await,
      "ADD_TO_PLAYLIST" => Err((
        VoiceDispatchErrorCode::Unsupported,
        "no playlist-mutation surface exists yet".into(),
      )),

      // device control
      "OPEN_WEBAPP" => self.open_webapp(slots).await,
      "SET_BRIGHTNESS" => self.set_brightness(slots).await,
      "SET_DISCOVERABLE" => {
        let enabled = slots.enabled.ok_or_else(|| bad("SET_DISCOVERABLE without `enabled`"))?;
        self
          .handle
          .bluetooth
          .profile_man
          .get()
          .await
          .set_discoverable(enabled)
          .await
          .map_err(|err| (VoiceDispatchErrorCode::Internal, format!("{err}")))?;
        Ok((VoiceDispatchTarget::Device, None))
      }
      "SYSTEM_ACTION" => {
        let action = slots
          .system_action
          .ok_or_else(|| bad("SYSTEM_ACTION without `systemAction`"))?;
        self.system_action(action).await
      }
      "PHONE_ACTION" => {
        let action = slots
          .phone_action
          .ok_or_else(|| bad("PHONE_ACTION without `phoneAction`"))?;
        self.phone_action(action).await
      }
      "CANCEL_INTERACTION" => {
        if let Err(err) = self.handle.state.mic.cancel().await {
          tracing::debug!("voice cancel: {err}");
        }
        Ok((VoiceDispatchTarget::Device, None))
      }

      // display-shaped: the active webapp renders these, or nothing does
      "SEARCH" => self.display(VoiceDisplayIntent::Search, resolved).await,
      "MORE_LIKE_THIS" => self.display(VoiceDisplayIntent::MoreLikeThis, resolved).await,
      "SHOW_VIEW" => {
        slots.view.ok_or_else(|| bad("SHOW_VIEW without `view`"))?;
        self.display(VoiceDisplayIntent::ShowView, resolved).await
      }

      // WHATS_PLAYING is answered from the daemon's own player mirror
      "WHATS_PLAYING" => Ok((VoiceDispatchTarget::Device, None)),

      "HELP" | "CLARIFY" | "NO_INTENT" => Err((
        VoiceDispatchErrorCode::NotDispatchable,
        format!("{} is resolved at the companion edge", resolved.intent),
      )),

      other => Err((
        VoiceDispatchErrorCode::Internal,
        format!("unknown intent {other}; schema drift between companion and daemon"),
      )),
    }
  }

  async fn play_catalog(&self, slots: &NluSlots) -> Routed {
    let uri = slots.uri.clone().ok_or((
      VoiceDispatchErrorCode::PlaybackFailed,
      "catalog play arrived without a resolved uri".into(),
    ))?;
    self
      .send_player(BridgeToGatewayPlayerMsgCommand::Play(gateway::PlayUri {
        uri,
        context: None,
      }))
      .await;
    Ok((VoiceDispatchTarget::Playback, None))
  }

  async fn set_volume(&self, slots: &NluSlots) -> Routed {
    if let Some(level) = slots.level {
      self
        .send_audio(BridgeToGatewayAudioMsgCommand::SetVolume(gateway::SetVolume {
          level: (level.min(100) as f32) / 100.0,
        }))
        .await;
      return Ok((VoiceDispatchTarget::Playback, None));
    }
    match slots
      .direction
      .ok_or_else(|| bad("SET_VOLUME without direction or level"))?
    {
      NluDirection::Up => self.handle.transport.volume_up().await,
      NluDirection::Down => self.handle.transport.volume_down().await,
    }
    Ok((VoiceDispatchTarget::Playback, None))
  }

  async fn set_brightness(&self, slots: &NluSlots) -> Routed {
    if let Some(NluBrightnessMode::Auto) = slots.brightness_mode {
      self
        .handle
        .state
        .als
        .set_mode(BrightnessMode::Auto)
        .await
        .map_err(|err| (VoiceDispatchErrorCode::Internal, format!("{err}")))?;
      return Ok((VoiceDispatchTarget::Device, None));
    }

    let target = if let Some(level) = slots.level {
      (level.min(100) as f32) / 100.0
    } else {
      let step = match slots.direction.ok_or_else(|| bad("SET_BRIGHTNESS without a target"))? {
        NluDirection::Up => BRIGHTNESS_STEP,
        NluDirection::Down => -BRIGHTNESS_STEP,
      };
      (self.handle.state.als.snapshot().await.brightness.level + step).clamp(0.0, 1.0)
    };

    self
      .handle
      .state
      .als
      .set_mode(BrightnessMode::Manual)
      .await
      .map_err(|err| (VoiceDispatchErrorCode::Internal, format!("{err}")))?;
    self
      .handle
      .state
      .als
      .set_level(target)
      .await
      .map_err(|err| (VoiceDispatchErrorCode::Internal, format!("{err}")))?
      .map_err(|err| (VoiceDispatchErrorCode::Internal, format!("{err:?}")))?;
    Ok((VoiceDispatchTarget::Device, None))
  }

  async fn play_preset(&self, slots: &NluSlots) -> Routed {
    let slot = preset_slot(slots)?;
    let presets = crate::stock::presets::list(&self.handle.state.kv)
      .await
      .map_err(|err| (VoiceDispatchErrorCode::Internal, format!("{err}")))?;
    let uri = presets
      .iter()
      .find(|p| p.slot_index == slot as usize)
      .map(|p| p.context_uri.clone())
      .ok_or((
        VoiceDispatchErrorCode::PlaybackFailed,
        format!("preset {slot} is empty"),
      ))?;
    self
      .send_player(BridgeToGatewayPlayerMsgCommand::Play(gateway::PlayUri {
        uri,
        context: None,
      }))
      .await;
    Ok((VoiceDispatchTarget::Playback, None))
  }

  async fn save_to_preset(&self, slots: &NluSlots) -> Routed {
    let slot = preset_slot(slots)?;
    let state = self.handle.state.player.state_reply().state;
    let context = state.context.ok_or((
      VoiceDispatchErrorCode::PlaybackFailed,
      "nothing is playing to save".into(),
    ))?;
    crate::stock::presets::upsert_many(
      &self.handle.state.kv,
      &[libbridgething::stock::StockPreset {
        slot_index: slot as usize,
        context_uri: context.uri,
        name: context.name,
        description: None,
        image_url: None,
      }],
    )
    .await
    .map_err(|err| (VoiceDispatchErrorCode::Internal, format!("{err}")))?;
    Ok((VoiceDispatchTarget::Device, None))
  }

  async fn favorite(&self, slots: &NluSlots, saved: bool) -> Routed {
    let uri = slots.uri.clone().ok_or((
      VoiceDispatchErrorCode::PlaybackFailed,
      "collection change arrived without a resolved uri".into(),
    ))?;
    let kind = if slots.podcast.is_some() {
      ItemKind::Show
    } else if slots.episode.is_some() {
      ItemKind::PodcastEpisode
    } else if slots.album.is_some() {
      ItemKind::Album
    } else if slots.artist.is_some() && slots.track.is_none() {
      ItemKind::Artist
    } else {
      ItemKind::Track
    };
    self.set_favorite(uri, kind, saved).await
  }

  async fn favorite_current(&self) -> Routed {
    let uri = self
      .handle
      .state
      .player
      .state_reply()
      .state
      .track
      .and_then(|t| t.uri)
      .ok_or((
        VoiceDispatchErrorCode::PlaybackFailed,
        "nothing is playing to favorite".into(),
      ))?;
    self.set_favorite(uri, ItemKind::Track, true).await
  }

  async fn set_favorite(&self, uri: String, kind: ItemKind, saved: bool) -> Routed {
    self
      .handle
      .bluetooth
      .gateway_man
      .broadcast_command(BridgeToGatewayLibraryMsgCommand::FavoritesSet(gateway::FavoritesSet {
        item: ItemRef {
          uri,
          kind,
          persistent_id: None,
        },
        liked: saved,
      }))
      .await;
    Ok((VoiceDispatchTarget::Playback, None))
  }

  async fn open_webapp(&self, slots: &NluSlots) -> Routed {
    let spoken = slots
      .webapp_name
      .as_deref()
      .ok_or_else(|| bad("OPEN_WEBAPP without `webappName`"))?;
    let id = match self.handle.state.webapps.resolve_by_name(spoken).await {
      Some(id) => id,
      None => {
        self.handle.state.webapps.rescan().await;
        self.handle.state.webapps.resolve_by_name(spoken).await.ok_or((
          VoiceDispatchErrorCode::WebappNotFound,
          format!("no installed webapp matches {spoken:?}"),
        ))?
      }
    };

    self
      .handle
      .state
      .set_active_webapp(id)
      .await
      .map_err(|err| (VoiceDispatchErrorCode::Internal, format!("{err}")))?;
    let url = navigate_url_for_active(&self.handle.state).await;
    if let Err(err) = self.handle.state.chrome.send(ChromeCommand::Navigate(url)).await {
      tracing::warn!("failed to reload kiosk after voice webapp switch: {err:?}");
    }
    self
      .handle
      .bluetooth
      .gateway_man
      .broadcast(gateway::BridgeToGatewayWebappMsgEvent::ActiveChanged(
        self.handle.state.active_webapp_changed_event().await,
      ))
      .await;
    Ok((VoiceDispatchTarget::WebappSwitch, Some(id)))
  }

  async fn display(&self, intent: VoiceDisplayIntent, resolved: &NluResolvedIntent) -> Routed {
    let active = self
      .handle
      .state
      .active_webapp()
      .await
      .map_err(|err| (VoiceDispatchErrorCode::Internal, format!("{err}")))?;
    let renders = match active {
      Some(id) => self
        .handle
        .state
        .webapps
        .manifest(id)
        .await
        .is_some_and(|m| m.renders_voice_display),
      None => false,
    };
    if !renders {
      return Err((
        VoiceDispatchErrorCode::NotDispatchable,
        "active webapp does not render voice display intents".into(),
      ));
    }

    let event = BridgeToClientVoiceMsgEvent::Intent(VoiceIntent {
      intent,
      slots: resolved.slots.clone(),
      transcript: resolved.transcript.clone(),
    });
    if let Err(errs) = self.handle.state.bus.broadcast_event(event).await {
      tracing::debug!("voice intent broadcast: {} non-fatal errors", errs.len());
    }
    Ok((VoiceDispatchTarget::Display, None))
  }

  async fn phone_action(&self, action: NluPhoneAction) -> Routed {
    let cmd = match action {
      NluPhoneAction::Answer => TelephonyManager::build_accept(0, None),
      NluPhoneAction::Decline | NluPhoneAction::End => TelephonyManager::build_end(0, None),
      NluPhoneAction::Hold => TelephonyManager::build_hold(true, None),
      NluPhoneAction::Unhold => TelephonyManager::build_hold(false, None),
      NluPhoneAction::Swap => TelephonyManager::build_hold(true, None),
      NluPhoneAction::Merge => TelephonyManager::build_accept(2, None),
      NluPhoneAction::Mute => TelephonyManager::build_mute(true),
      NluPhoneAction::Unmute => TelephonyManager::build_mute(false),
    };
    self
      .handle
      .state
      .telephony
      .dispatch(cmd)
      .await
      .map_err(|err| (VoiceDispatchErrorCode::Internal, format!("{err}")))?;
    Ok((VoiceDispatchTarget::Phone, None))
  }

  async fn system_action(&self, action: NluSystemAction) -> Routed {
    match action {
      NluSystemAction::Reboot => power::reboot().await,
      NluSystemAction::PowerOff => power::power_off().await,
    }
    .map_err(|err| (VoiceDispatchErrorCode::Internal, format!("{err}")))?;
    Ok((VoiceDispatchTarget::Device, None))
  }

  async fn send_player(&self, msg: BridgeToGatewayPlayerMsgCommand) {
    self.handle.bluetooth.gateway_man.broadcast_command(msg).await;
  }

  async fn send_audio(&self, msg: BridgeToGatewayAudioMsgCommand) {
    self.handle.bluetooth.gateway_man.broadcast_command(msg).await;
  }
}

fn has_catalog_slots(slots: &NluSlots) -> bool {
  slots.artist.is_some()
    || slots.track.is_some()
    || slots.album.is_some()
    || slots.playlist.is_some()
    || slots.podcast.is_some()
    || slots.episode.is_some()
    || slots.mood.is_some()
    || slots.genre.is_some()
    || slots.era.is_some()
    || slots.uri.is_some()
}

fn preset_slot(slots: &NluSlots) -> Result<u8, (VoiceDispatchErrorCode, String)> {
  slots
    .preset
    .as_deref()
    .and_then(|p| p.parse::<u8>().ok())
    .filter(|n| (1..=4).contains(n))
    .ok_or_else(|| bad("preset must be 1-4"))
}

fn repeat_mode(mode: NluRepeatMode) -> RepeatMode {
  match mode {
    NluRepeatMode::Off => RepeatMode::Off,
    NluRepeatMode::All => RepeatMode::All,
    NluRepeatMode::One => RepeatMode::One,
  }
}

fn playback_speed(speed: NluPlaybackSpeed) -> f32 {
  match speed {
    NluPlaybackSpeed::One => 1.0,
    NluPlaybackSpeed::OnePointTwo => 1.2,
    NluPlaybackSpeed::OnePointFive => 1.5,
    NluPlaybackSpeed::Two => 2.0,
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn slots() -> NluSlots {
    NluSlots::default()
  }

  #[test]
  fn bare_play_is_not_a_catalog_play() {
    assert!(!has_catalog_slots(&slots()));
  }

  #[test]
  fn any_entity_slot_makes_it_a_catalog_play() {
    let mut s = slots();
    s.artist = Some("mitski".into());
    assert!(has_catalog_slots(&s));
  }

  #[test]
  fn preset_rejects_out_of_range() {
    let mut s = slots();
    s.preset = Some("5".into());
    assert!(preset_slot(&s).is_err());
    s.preset = Some("3".into());
    assert_eq!(preset_slot(&s).unwrap(), 3);
  }
}
