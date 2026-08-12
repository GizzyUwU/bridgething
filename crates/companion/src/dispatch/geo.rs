use std::{
  sync::{Arc, Mutex},
  time::Duration,
};

use bridgething_gateway::{GeoHandler, HandlerError, OutboundLink, OutboundLinkExt, Reply};
use libbridgething::{
  GeoAccuracy as WireAccuracy, GeoError as WireGeoError, Position as WirePosition,
  gateway::{GatewayToBridgeGeoMsgEvent, GeoErrorReply, GeoGetOnce, GeoGetOnceReply, GeoWatch},
  wire::WireError,
};
use tokio::sync::{mpsc::UnboundedReceiver, oneshot};
use uuid::Uuid;

use crate::{
  backend::{GeoAccuracy, GeoError, GeoEvent, GeoInbox, GeoProvider, Position},
  dispatch::{Relay, ask, tell},
};

const ONE_SHOT_TIMEOUT: Duration = Duration::from_secs(30);

struct OneShot {
  id: Uuid,
  reply: oneshot::Sender<Result<Position, GeoError>>,
}

#[derive(Default)]
struct GeoState {
  watching: bool,
  watch_accuracy: Option<WireAccuracy>,
  one_shots: Vec<OneShot>,
}

pub struct GeoDispatcher {
  provider: Arc<dyn GeoProvider>,
  link: Arc<dyn OutboundLink>,
  authorization: Arc<dyn Fn(bool) + Send + Sync>,
  one_shot_timeout: Duration,
  state: Arc<Mutex<GeoState>>,
  relay: Relay,
}

impl GeoDispatcher {
  pub fn new(
    provider: Arc<dyn GeoProvider>,
    link: Arc<dyn OutboundLink>,
    authorization: Arc<dyn Fn(bool) + Send + Sync>,
  ) -> Self {
    Self {
      provider,
      link,
      authorization,
      one_shot_timeout: ONE_SHOT_TIMEOUT,
      state: Arc::new(Mutex::new(GeoState::default())),
      relay: Relay::default(),
    }
  }

  pub fn with_one_shot_timeout(mut self, timeout: Duration) -> Self {
    self.one_shot_timeout = timeout;
    self
  }

  pub async fn start(&self) {
    let (inbox, events) = GeoInbox::channel();
    self.relay.hold(tokio::spawn(relay(
      events,
      self.link.clone(),
      self.state.clone(),
      self.authorization.clone(),
    )));
    tell(&self.provider, move |provider| provider.start(inbox)).await;
    (self.authorization)(self.can_provide_location().await);
  }

  pub async fn stop(&self) {
    let (was_watching, abandoned) = {
      let mut held = self.state.lock().unwrap();
      held.watch_accuracy = None;
      (
        std::mem::replace(&mut held.watching, false),
        std::mem::take(&mut held.one_shots),
      )
    };
    if was_watching {
      tell(&self.provider, |provider| provider.stop_updating()).await;
    }
    if !abandoned.is_empty() {
      tell(&self.provider, |provider| provider.cancel_once()).await;
    }
    for shot in abandoned {
      let _ = shot.reply.send(Err(GeoError::Unavailable));
    }
    self.relay.release();
    tell(&self.provider, |provider| provider.stop()).await;
  }

  pub async fn can_provide_location(&self) -> bool {
    ask(&self.provider, |provider| provider.can_provide_location())
      .await
      .unwrap_or(false)
  }

  async fn prepare(&self, accuracy: WireAccuracy) {
    let accuracy = provider_accuracy(accuracy);
    tell(&self.provider, move |provider| {
      provider.request_authorization();
      provider.configure(accuracy);
    })
    .await;
  }

  async fn expire(&self, id: Uuid) {
    let withdraw = {
      let mut held = self.state.lock().unwrap();
      match held.one_shots.iter().position(|shot| shot.id == id) {
        Some(at) => {
          held.one_shots.remove(at);
          held.one_shots.is_empty()
        }
        None => false,
      }
    };
    if withdraw {
      tell(&self.provider, |provider| provider.cancel_once()).await;
    }
  }
}

impl GeoHandler for GeoDispatcher {
  async fn get_once(&self, request: GeoGetOnce) -> Result<Reply<GeoGetOnceReply>, HandlerError<GeoErrorReply>> {
    if !self.can_provide_location().await {
      return Err(refused(WireGeoError::PermissionDenied));
    }
    self.prepare(request.accuracy).await;

    let id = Uuid::now_v7();
    let (reply, answer) = oneshot::channel();
    self.state.lock().unwrap().one_shots.push(OneShot { id, reply });
    tell(&self.provider, |provider| provider.request_once()).await;

    match tokio::time::timeout(self.one_shot_timeout, answer).await {
      Ok(Ok(Ok(position))) => Ok(
        GeoGetOnceReply {
          position: wire_position(position),
        }
        .into(),
      ),
      Ok(Ok(Err(error))) => Err(refused(wire_error(error))),
      Ok(Err(_)) => Err(refused(WireGeoError::Unavailable)),
      Err(_) => {
        tracing::warn!("geo.getOnce timed out with no fix and no error");
        self.expire(id).await;
        Err(refused(WireGeoError::Unavailable))
      }
    }
  }

  async fn watch(&self, payload: GeoWatch) -> Result<(), WireError> {
    if !self.can_provide_location().await {
      let _ = self
        .link
        .event(GatewayToBridgeGeoMsgEvent::ErrorEvent(GeoErrorReply {
          error: WireGeoError::PermissionDenied,
        }))
        .await;
      return Ok(());
    }
    self.prepare(payload.accuracy).await;

    let (start, restart) = {
      let mut held = self.state.lock().unwrap();
      let was_watching = std::mem::replace(&mut held.watching, true);
      let previous = held.watch_accuracy.replace(payload.accuracy);
      (!was_watching, was_watching && previous != Some(payload.accuracy))
    };
    if restart {
      tell(&self.provider, |provider| provider.stop_updating()).await;
      tell(&self.provider, |provider| provider.start_updating()).await;
    } else if start {
      tell(&self.provider, |provider| provider.start_updating()).await;
    }
    Ok(())
  }

  async fn unwatch(&self) -> Result<(), WireError> {
    let stop = {
      let mut held = self.state.lock().unwrap();
      held.watch_accuracy = None;
      std::mem::replace(&mut held.watching, false)
    };
    if stop {
      tell(&self.provider, |provider| provider.stop_updating()).await;
    }
    Ok(())
  }
}

async fn relay(
  mut events: UnboundedReceiver<GeoEvent>,
  link: Arc<dyn OutboundLink>,
  state: Arc<Mutex<GeoState>>,
  authorization: Arc<dyn Fn(bool) + Send + Sync>,
) {
  while let Some(event) = events.recv().await {
    match event {
      GeoEvent::Position(position) => {
        let watching = {
          let mut held = state.lock().unwrap();
          if !held.one_shots.is_empty() {
            let _ = held.one_shots.remove(0).reply.send(Ok(position));
          }
          held.watching
        };
        if watching {
          let _ = link
            .event(GatewayToBridgeGeoMsgEvent::Position(wire_position(position)))
            .await;
        }
      }
      GeoEvent::Failed(error) => {
        let (waiting, watching) = {
          let mut held = state.lock().unwrap();
          (std::mem::take(&mut held.one_shots), held.watching)
        };
        for shot in waiting {
          let _ = shot.reply.send(Err(error));
        }
        if watching {
          tracing::warn!(?error, "geo watch failed while subscribed");
          let _ = link
            .event(GatewayToBridgeGeoMsgEvent::ErrorEvent(GeoErrorReply {
              error: wire_error(error),
            }))
            .await;
        }
      }
      GeoEvent::AuthorizationChanged { granted } => authorization(granted),
    }
  }
}

fn refused(error: WireGeoError) -> HandlerError<GeoErrorReply> {
  HandlerError::Domain(GeoErrorReply { error })
}

fn provider_accuracy(accuracy: WireAccuracy) -> GeoAccuracy {
  match accuracy {
    WireAccuracy::Coarse => GeoAccuracy::Coarse,
    WireAccuracy::Fine => GeoAccuracy::Fine,
  }
}

fn wire_error(error: GeoError) -> WireGeoError {
  match error {
    GeoError::PermissionDenied => WireGeoError::PermissionDenied,
    GeoError::NotDeclared => WireGeoError::NotDeclared,
    GeoError::Unavailable => WireGeoError::Unavailable,
    GeoError::UnknownToken => WireGeoError::UnknownToken,
  }
}

fn wire_position(position: Position) -> WirePosition {
  WirePosition {
    lat: position.lat,
    lon: position.lon,
    alt_m: position.alt_m,
    accuracy_m: position.accuracy_m,
    speed_mps: position.speed_mps,
    heading_deg: position.heading_deg,
    ts_unix_s: position.ts_unix_s,
  }
}
