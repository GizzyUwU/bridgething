use libbridgething::client::ClientInteractionCommand;

use super::{HandlerResult, MsgHandle};

#[derive(Debug)]
pub struct InteractionHandler {
  handle: MsgHandle,
}

impl InteractionHandler {
  pub fn new(handle: MsgHandle) -> Self {
    Self { handle }
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
      ClientInteractionCommand::IncreaseVolume => Ok(self.handle.transport.volume_up().await?),
      ClientInteractionCommand::DecreaseVolume => Ok(self.handle.transport.volume_down().await?),
      ClientInteractionCommand::MuteToggle => Ok(self.handle.transport.mute_toggle().await?),
      ClientInteractionCommand::SkipToIndex { index } => Ok(self.handle.transport.skip_to_index(index).await?),
      ClientInteractionCommand::SkipNext => Ok(self.handle.transport.next().await?),
      ClientInteractionCommand::SkipPrev => Ok(self.handle.transport.prev().await?),
      ClientInteractionCommand::SeekTo { position_ms } => Ok(self.handle.transport.seek_to(position_ms).await?),
      ClientInteractionCommand::Pause => Ok(self.handle.transport.pause().await?),
      ClientInteractionCommand::Resume => Ok(self.handle.transport.play().await?),
      ClientInteractionCommand::SetShuffle { shuffle } => Ok(self.handle.transport.set_shuffle(shuffle).await?),
      ClientInteractionCommand::SetRepeat { repeat_mode } => Ok(self.handle.transport.set_repeat(repeat_mode).await?),
    }
  }

  async fn phone_answer(&self) -> HandlerResult {
    tracing::debug!("({}) answering phone", &self.handle.from);
    Ok(())
  }

  async fn phone_decline(&self) -> HandlerResult {
    tracing::debug!("({}) declining phone", &self.handle.from);
    Ok(())
  }

  async fn phone_call_image(&self, phone_number: String) -> HandlerResult {
    tracing::debug!(
      "({}) getting phone call image for number: {}",
      &self.handle.id,
      phone_number
    );
    Ok(())
  }

  async fn phone_call_message(&self, phone_number: String, message: String) -> HandlerResult {
    tracing::debug!(
      "({}) sending phone call message to number: {}, message: {}",
      &self.handle.id,
      phone_number,
      message
    );
    Ok(())
  }


}
