//! Apple Media Service (AMS) GATT client, as a consumer of the shared
//! LE session. Reads the iPhone's media-player volume over the same
//! LE bond ANCS uses. Read-only: AMS feeds the volume level into
//! `AudioManager`; it is not a second now-playing source (iAP2
//! NowPlaying owns playback state).
//!
//! Volume actuation is HID over iAP2; AMS is purely how the accessory
//! learns the current level. iOS has no separate mute state, so a level
//! of 0/16 is mute; `muted` is always reported false and the level
//! carries the signal.
//!
//! Subscribing the Entity Update characteristic and registering the
//! Player/Volume attribute makes iOS push the current value immediately
//! as the first notification, then stream every change after. No
//! separate one-shot read is needed.

use std::pin::Pin;

use bluer::gatt::{
  WriteOp,
  remote::{Characteristic, CharacteristicWriteRequest, Service},
};
use futures::Stream;
use uuid::Uuid;

pub const AMS_SERVICE: Uuid = Uuid::from_u128(0x89D3502B_0F36_433A_8EF4_C502AD55F8DC);
const ENTITY_UPDATE: Uuid = Uuid::from_u128(0x2F7CABCE_808D_411F_9A0C_BB92BA96C102);

const ENTITY_PLAYER: u8 = 0x00;
const PLAYER_ATTR_VOLUME: u8 = 0x02;

pub type EntityUpdateStream = Pin<Box<dyn Stream<Item = Vec<u8>> + Send>>;

pub async fn subscribe(service: &Service) -> AmsResult<EntityUpdateStream> {
  let entity_update = locate(service, ENTITY_UPDATE)
    .await?
    .ok_or(AmsError::CharacteristicMissing(ENTITY_UPDATE))?;
  let stream = entity_update.notify().await?;
  entity_update
    .write_ext(
      &[ENTITY_PLAYER, PLAYER_ATTR_VOLUME],
      &CharacteristicWriteRequest {
        op_type: WriteOp::Request,
        ..Default::default()
      },
    )
    .await?;
  Ok(Box::pin(stream))
}

/// frame: `[entity, attr, flags, value-bytes...]`. player volume is an ascii `0.0`-`1.0` string
pub fn parse_volume(frame: &[u8]) -> Option<f32> {
  if frame.len() < 4 {
    return None;
  }
  if frame[0] != ENTITY_PLAYER || frame[1] != PLAYER_ATTR_VOLUME {
    return None;
  }
  let level = std::str::from_utf8(&frame[3..]).ok()?.trim().parse::<f32>().ok()?;
  Some(level.clamp(0.0, 1.0))
}

async fn locate(service: &Service, uuid: Uuid) -> AmsResult<Option<Characteristic>> {
  for ch in service.characteristics().await? {
    if ch.uuid().await? == uuid {
      return Ok(Some(ch));
    }
  }
  Ok(None)
}

pub type AmsResult<T> = Result<T, AmsError>;

#[derive(Debug, thiserror::Error)]
pub enum AmsError {
  #[error("AMS characteristic {0} not found")]
  CharacteristicMissing(Uuid),
  #[error(transparent)]
  Bluer(#[from] bluer::Error),
}
