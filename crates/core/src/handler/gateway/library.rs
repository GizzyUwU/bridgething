use libbridgething::{
  client::{BridgeToClientLibraryMsgEvent, FavoriteChanged},
  gateway::GatewayToBridgeLibraryMsgEvent,
};

use super::{HandlerResult, MsgHandle};

pub struct LibraryHandler {
  handle: MsgHandle,
}

impl LibraryHandler {
  pub fn new(handle: MsgHandle) -> Self {
    Self { handle }
  }

  pub async fn handle(self, msg: GatewayToBridgeLibraryMsgEvent) -> HandlerResult {
    match msg {
      GatewayToBridgeLibraryMsgEvent::FavoriteChanged(change) => {
        self
          .handle
          .state
          .bus
          .broadcast_event(BridgeToClientLibraryMsgEvent::FavoriteChanged(FavoriteChanged {
            uri: change.uri,
            liked: change.liked,
          }))
          .await?;
      }
    }
    Ok(())
  }
}
