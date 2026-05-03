use libbridgething::gateway::GatewayToBridgeLibraryMsg;

use super::{HandlerResult, MsgHandle};

pub struct LibraryHandler {
  handle: MsgHandle,
}

impl LibraryHandler {
  pub fn new(handle: MsgHandle) -> Self {
    Self { handle }
  }

  pub async fn handle(self, msg: GatewayToBridgeLibraryMsg) -> HandlerResult {
    match msg {
      GatewayToBridgeLibraryMsg::BrowseReply(_) => self.handle.unimplemented("gateway:library.browseReply").await,
      GatewayToBridgeLibraryMsg::SearchReply(_) => self.handle.unimplemented("gateway:library.searchReply").await,
      GatewayToBridgeLibraryMsg::RecommendationsReply(_) => {
        self.handle.unimplemented("gateway:library.recommendationsReply").await
      }
      GatewayToBridgeLibraryMsg::FavoritesListReply(_) => {
        self.handle.unimplemented("gateway:library.favoritesListReply").await
      }
      GatewayToBridgeLibraryMsg::LibraryErrorReply(_) => {
        self.handle.unimplemented("gateway:library.libraryErrorReply").await
      }
      GatewayToBridgeLibraryMsg::FavoriteChanged(_) => {
        self.handle.unimplemented("gateway:library.favoriteChanged").await
      }
    }
    Ok(())
  }
}
