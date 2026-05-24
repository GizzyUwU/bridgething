use libbridgething::{
  client::{BridgeToClientLibraryMsgEvent, FavoriteChanged as ClientFavoriteChanged},
  gateway::{FavoriteChanged, GatewayToBridgeLibraryMsgEventDispatch},
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
}
