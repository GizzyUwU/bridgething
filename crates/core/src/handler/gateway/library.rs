use libbridgething::{
  client::{BridgeToClientLibraryMsgEvent, FavoriteChanged as ClientFavoriteChanged},
  gateway::{FavoriteChanged, GatewayToBridgeLibraryMsgEventDispatch, LibraryChanged},
};

use super::{HandlerResult, MsgHandle};

pub struct LibraryHandler {
  handle: MsgHandle,
}

impl LibraryHandler {
  pub fn new(handle: MsgHandle) -> Self {
    Self { handle }
  }
}

impl GatewayToBridgeLibraryMsgEventDispatch for LibraryHandler {
  type Output = HandlerResult;

  async fn favorite_changed(&self, params: FavoriteChanged) -> HandlerResult {
    self
      .handle
      .state
      .bus
      .broadcast_event(BridgeToClientLibraryMsgEvent::FavoriteChanged(ClientFavoriteChanged {
        uri: params.uri,
        liked: params.liked,
      }))
      .await?;
    Ok(())
  }

  async fn library_changed(&self, params: LibraryChanged) -> HandlerResult {
    tracing::debug!(scope = ?params.scope, "gateway reported a library change; invalidating cached home");
    self.handle.state.player.note_library_changed().await?;
    Ok(())
  }
}
