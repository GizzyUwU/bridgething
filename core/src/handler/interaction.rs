use libbridgething::client::ClientInteractionCommand;

use crate::state::State;

use super::{Handler, HandlerResult, MsgHandle};

#[derive(Debug)]
pub struct InteractionHandler<'a> {
  handle: MsgHandle,
  state: &'a mut State,
}

impl<'a> InteractionHandler<'a> {
  pub fn new(handler: Handler<'a>) -> Self {
    Self {
      handle: handler.handle,
      state: handler.state,
    }
  }

  pub async fn handle(self, msg: ClientInteractionCommand) -> HandlerResult {
    tracing::debug!(
      "({}) handling interaction message: id: {:?}; stock_msg_id: {:?}",
      &self.handle.from,
      &self.handle.id,
      &self.handle.stock_msg_id
    );

    match msg {
      ClientInteractionCommand::PhoneAnswer => self.phone_answer().await,
      ClientInteractionCommand::PhoneDecline => self.phone_decline().await,
      ClientInteractionCommand::PhoneCallImage { phone_number } => self.phone_call_image(phone_number).await,
      ClientInteractionCommand::PhoneCallMessage { phone_number, message } => {
        self.phone_call_message(phone_number, message).await
      }
      ClientInteractionCommand::IncreaseVolume => self.increase_volume().await,
      ClientInteractionCommand::DecreaseVolume => self.decrease_volume().await,
      ClientInteractionCommand::SkipToIndex { index } => self.skip_to_index(index).await,
      ClientInteractionCommand::SkipNext => self.skip_next().await,
      ClientInteractionCommand::SkipPrev { allow_seeking } => self.skip_prev(allow_seeking).await,
      ClientInteractionCommand::SeekTo { position } => self.seek_to(position).await,
      ClientInteractionCommand::Pause => self.pause().await,
      ClientInteractionCommand::Resume => self.resume().await,
      ClientInteractionCommand::SetShuffle { shuffle } => self.set_shuffle(shuffle).await,
      ClientInteractionCommand::SetRepeat { repeat_mode } => self.set_repeat(repeat_mode).await,
    }
  }

  async fn phone_answer(&self) -> HandlerResult {
    tracing::debug!("({}) answering phone", &self.handle.from);
    // Ok(self.handle.respond().await?)
    Ok(())
  }

  async fn phone_decline(&self) -> HandlerResult {
    tracing::debug!("({}) declining phone", &self.handle.from);
    // Ok(self.handle.respond().await?)
    Ok(())
  }

  async fn phone_call_image(&self, phone_number: String) -> HandlerResult {
    tracing::debug!(
      "({}) getting phone call image for number: {}",
      &self.handle.id,
      phone_number
    );
    // Ok(self.handle.respond().await?)
    Ok(())
  }

  async fn phone_call_message(&self, phone_number: String, message: String) -> HandlerResult {
    tracing::debug!(
      "({}) sending phone call message to number: {}, message: {}",
      &self.handle.id,
      phone_number,
      message
    );
    // Ok(self.handle.respond().await?)
    Ok(())
  }

  async fn increase_volume(&self) -> HandlerResult {
    tracing::debug!("({}) increasing volume", &self.handle.from);
    // Ok(self.handle.respond().await?)
    Ok(())
  }

  async fn decrease_volume(&self) -> HandlerResult {
    tracing::debug!("({}) decreasing volume", &self.handle.from);
    // Ok(self.handle.respond().await?)
    Ok(())
  }

  async fn skip_to_index(&self, index: usize) -> HandlerResult {
    tracing::debug!("({}) skipping to index: {}", &self.handle.from, index);
    // Ok(self.handle.respond().await?)
    Ok(())
  }

  async fn skip_next(&self) -> HandlerResult {
    tracing::debug!("({}) skipping to next track", &self.handle.from);
    let Some(player) = &self.state.player else {
      return Ok(());
    };

    Ok(player.next().await?)
  }

  async fn skip_prev(&self, allow_seeking: bool) -> HandlerResult {
    tracing::debug!(
      "({}) skipping to previous track, allow seeking: {}",
      &self.handle.id,
      allow_seeking
    );
    let Some(player) = &self.state.player else {
      return Ok(());
    };

    Ok(player.prev().await?)
  }

  async fn seek_to(&self, position: usize) -> HandlerResult {
    tracing::debug!("({}) seeking to position: {}", &self.handle.from, position);
    // Ok(self.handle.respond().await?)
    Ok(())
  }

  async fn pause(&self) -> HandlerResult {
    tracing::debug!("({}) pausing playback", &self.handle.from);
    let Some(player) = &self.state.player else {
      return Ok(());
    };

    Ok(player.pause().await?)
  }

  async fn resume(&self) -> HandlerResult {
    tracing::debug!("({}) resuming playback", &self.handle.from);
    let Some(player) = &self.state.player else {
      return Ok(());
    };

    Ok(player.play().await?)
  }

  async fn set_shuffle(&self, shuffle: bool) -> HandlerResult {
    tracing::debug!("({}) setting shuffle to: {}", &self.handle.from, shuffle);
    let Some(player) = &self.state.player else {
      return Ok(());
    };

    Ok(player.shuffle(shuffle.into()).await?)
  }

  async fn set_repeat(&self, repeat: bool) -> HandlerResult {
    tracing::debug!("({}) setting repeat to: {}", &self.handle.from, repeat);
    let Some(player) = &self.state.player else {
      return Ok(());
    };

    Ok(player.repeat(repeat.into()).await?)
  }
}
