use libbridgething::{
  PlayerError,
  client::{
    BridgeToClientPlayerMsg, ClientToBridgePlayerMsg, PlayUri, PlayerErrorReply, PlayerQueueGet, PlayerStateGet,
    QueueUri, SeekTo, SetRepeat, SetShuffle, SkipToIndex,
  },
  gateway::{self, BridgeToGatewayPlayerMsgCommand},
};

use super::{HandlerResult, MsgHandle};

pub struct PlayerHandler {
  handle: MsgHandle,
}

impl PlayerHandler {
  pub fn new(handle: MsgHandle) -> Self {
    Self { handle }
  }

  pub async fn handle(self, msg: ClientToBridgePlayerMsg) -> HandlerResult {
    let transport = self.handle.transport.clone();
    match msg {
      ClientToBridgePlayerMsg::Play(req) => self.play(req).await,
      ClientToBridgePlayerMsg::Queue(req) => self.queue(req).await,
      ClientToBridgePlayerMsg::Pause => Ok(transport.pause().await?),
      ClientToBridgePlayerMsg::Resume => Ok(transport.play().await?),
      ClientToBridgePlayerMsg::SkipNext => Ok(transport.next().await?),
      ClientToBridgePlayerMsg::SkipPrev => Ok(transport.prev().await?),
      ClientToBridgePlayerMsg::SkipToIndex(SkipToIndex { index }) => Ok(transport.skip_to_index(index).await?),
      ClientToBridgePlayerMsg::SeekTo(SeekTo { position_ms }) => Ok(transport.seek_to(position_ms).await?),
      ClientToBridgePlayerMsg::SetShuffle(SetShuffle { on }) => Ok(transport.set_shuffle(on).await?),
      ClientToBridgePlayerMsg::SetRepeat(SetRepeat { mode }) => Ok(transport.set_repeat(mode).await?),
      ClientToBridgePlayerMsg::SetSpeed(req) => {
        self
          .forward_command(BridgeToGatewayPlayerMsgCommand::SetSpeed(gateway::SetSpeed {
            speed: req.speed,
          }))
          .await
      }
      ClientToBridgePlayerMsg::SetCrossfade(req) => {
        self
          .forward_command(BridgeToGatewayPlayerMsgCommand::SetCrossfade(gateway::SetCrossfade {
            duration_ms: req.duration_ms,
          }))
          .await
      }
      ClientToBridgePlayerMsg::StateGet => self.state_get().await,
      ClientToBridgePlayerMsg::QueueGet => self.queue_get().await,
    }
  }

  async fn play(self, PlayUri { uri, context }: PlayUri) -> HandlerResult {
    let snapshot = self.handle.state.capabilities.snapshot();
    let Some(scheme) = scheme_of(&uri) else {
      return self
        .respond_player_error(PlayerError::SchemeUnclaimed { scheme: uri })
        .await;
    };
    if snapshot.gateway.is_none() {
      return self.respond_player_error(PlayerError::NoGateway).await;
    }
    if !snapshot.uri_schemes.iter().any(|s| s == &scheme) {
      return self.respond_player_error(PlayerError::SchemeUnclaimed { scheme }).await;
    }
    self
      .forward_command(BridgeToGatewayPlayerMsgCommand::Play(gateway::PlayUri { uri, context }))
      .await
  }

  async fn queue(self, QueueUri { uri, position }: QueueUri) -> HandlerResult {
    let snapshot = self.handle.state.capabilities.snapshot();
    let Some(scheme) = scheme_of(&uri) else {
      return self
        .respond_player_error(PlayerError::SchemeUnclaimed { scheme: uri })
        .await;
    };
    if snapshot.gateway.is_none() {
      return self.respond_player_error(PlayerError::NoGateway).await;
    }
    if !snapshot.uri_schemes.iter().any(|s| s == &scheme) {
      return self.respond_player_error(PlayerError::SchemeUnclaimed { scheme }).await;
    }
    self
      .forward_command(BridgeToGatewayPlayerMsgCommand::Queue(gateway::QueueUri {
        uri,
        position,
      }))
      .await
  }

  async fn state_get(self) -> HandlerResult {
    let reply = self.handle.state.player.state_reply().await;
    self.handle.respond_to::<PlayerStateGet>(reply).await?;
    Ok(())
  }

  async fn queue_get(self) -> HandlerResult {
    let reply = self.handle.state.player.queue_reply().await;
    self.handle.respond_to::<PlayerQueueGet>(reply).await?;
    Ok(())
  }

  async fn forward_command<C>(self, cmd: C) -> HandlerResult
  where
    C: libbridgething::wire::WireCommand<libbridgething::gateway::BridgeToGatewayMsgData>,
  {
    self.handle.bluetooth.gateway_man.broadcast_command(cmd).await;
    Ok(())
  }

  async fn respond_player_error(self, error: PlayerError) -> HandlerResult {
    self
      .handle
      .respond(BridgeToClientPlayerMsg::ErrorReply(PlayerErrorReply { error }))
      .await?;
    Ok(())
  }
}

fn scheme_of(uri: &str) -> Option<String> {
  let (head, _) = uri.split_once(':')?;
  if head.is_empty() {
    return None;
  }
  let mut chars = head.chars();
  let first = chars.next()?;
  if !first.is_ascii_alphabetic() {
    return None;
  }
  if !chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.')) {
    return None;
  }
  Some(head.to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
  use super::scheme_of;

  #[test]
  fn parses_simple_scheme() {
    assert_eq!(scheme_of("spotify:track:abc"), Some("spotify".into()));
  }

  #[test]
  fn lowercases() {
    assert_eq!(scheme_of("Spotify:track:abc"), Some("spotify".into()));
  }

  #[test]
  fn rejects_no_colon() {
    assert_eq!(scheme_of("spotifytrackabc"), None);
  }

  #[test]
  fn rejects_leading_digit() {
    assert_eq!(scheme_of("4spotify:track:abc"), None);
  }

  #[test]
  fn allows_compound_chars() {
    assert_eq!(scheme_of("apple-music:track:abc"), Some("apple-music".into()));
  }
}
