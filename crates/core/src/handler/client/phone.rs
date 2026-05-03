use libbridgething::client::ClientToBridgePhoneMsg;

use super::{HandlerResult, MsgHandle};

pub struct PhoneHandler {
  handle: MsgHandle,
}

impl PhoneHandler {
  pub fn new(handle: MsgHandle) -> Self {
    Self { handle }
  }

  pub async fn handle(self, msg: ClientToBridgePhoneMsg) -> HandlerResult {
    match msg {
      ClientToBridgePhoneMsg::Answer(_) => Ok(self.handle.unimplemented("phone.answer").await?),
      ClientToBridgePhoneMsg::Decline(_) => Ok(self.handle.unimplemented("phone.decline").await?),
      ClientToBridgePhoneMsg::End(_) => Ok(self.handle.unimplemented("phone.end").await?),
      ClientToBridgePhoneMsg::Hold(_) => Ok(self.handle.unimplemented("phone.hold").await?),
      ClientToBridgePhoneMsg::Unhold(_) => Ok(self.handle.unimplemented("phone.unhold").await?),
      ClientToBridgePhoneMsg::StateGet => Ok(self.handle.unimplemented("phone.stateGet").await?),
    }
  }
}
