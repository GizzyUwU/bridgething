use libbridgething::client::{ClientToBridgePlayerMsg, SeekTo, SetRepeat, SetShuffle, SkipToIndex};

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
      ClientToBridgePlayerMsg::Play(_) => Ok(self.handle.unimplemented("player.play").await?),
      ClientToBridgePlayerMsg::Queue(_) => Ok(self.handle.unimplemented("player.queue").await?),
      ClientToBridgePlayerMsg::Pause => Ok(transport.pause().await?),
      ClientToBridgePlayerMsg::Resume => Ok(transport.play().await?),
      ClientToBridgePlayerMsg::SkipNext => Ok(transport.next().await?),
      ClientToBridgePlayerMsg::SkipPrev => Ok(transport.prev().await?),
      ClientToBridgePlayerMsg::SkipToIndex(SkipToIndex { index }) => Ok(transport.skip_to_index(index).await?),
      ClientToBridgePlayerMsg::SeekTo(SeekTo { position_ms }) => Ok(transport.seek_to(position_ms).await?),
      ClientToBridgePlayerMsg::SetShuffle(SetShuffle { on }) => Ok(transport.set_shuffle(on).await?),
      ClientToBridgePlayerMsg::SetRepeat(SetRepeat { mode }) => Ok(transport.set_repeat(mode).await?),
      ClientToBridgePlayerMsg::SetSpeed(_) => Ok(self.handle.unimplemented("player.setSpeed").await?),
      ClientToBridgePlayerMsg::SetCrossfade(_) => Ok(self.handle.unimplemented("player.setCrossfade").await?),
      ClientToBridgePlayerMsg::StateGet => Ok(self.handle.unimplemented("player.stateGet").await?),
      ClientToBridgePlayerMsg::QueueGet => Ok(self.handle.unimplemented("player.queueGet").await?),
    }
  }
}
