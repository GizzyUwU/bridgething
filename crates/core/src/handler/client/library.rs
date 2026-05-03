use libbridgething::client::ClientToBridgeLibraryMsg;

use super::{HandlerResult, MsgHandle};

pub struct LibraryHandler {
  handle: MsgHandle,
}

impl LibraryHandler {
  pub fn new(handle: MsgHandle) -> Self {
    Self { handle }
  }

  pub async fn handle(self, msg: ClientToBridgeLibraryMsg) -> HandlerResult {
    match msg {
      ClientToBridgeLibraryMsg::Browse(_) => Ok(self.handle.unimplemented("library.browse").await?),
      ClientToBridgeLibraryMsg::Search(_) => Ok(self.handle.unimplemented("library.search").await?),
      ClientToBridgeLibraryMsg::Recommendations(_) => Ok(self.handle.unimplemented("library.recommendations").await?),
      ClientToBridgeLibraryMsg::FavoritesList(_) => Ok(self.handle.unimplemented("library.favoritesList").await?),
      ClientToBridgeLibraryMsg::FavoritesToggle(_) => Ok(self.handle.unimplemented("library.favoritesToggle").await?),
      ClientToBridgeLibraryMsg::FavoritesSet(_) => Ok(self.handle.unimplemented("library.favoritesSet").await?),
    }
  }
}
