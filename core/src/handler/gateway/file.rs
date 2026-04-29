use libbridgething::gateway::{FileResponseData, GatewayToBridgeFileMsg};

use super::{HandlerResult, MsgHandle};

#[derive(Debug)]
pub struct FileHandler {
  handle: MsgHandle,
}

impl FileHandler {
  pub fn new(handle: MsgHandle) -> Self {
    Self { handle }
  }

  pub async fn handle(&mut self, msg: GatewayToBridgeFileMsg) -> HandlerResult {
    tracing::debug!("({:?}) handling file message", &self.handle.address);

    match msg {
      GatewayToBridgeFileMsg::FileResponse(FileResponseData { file }) => {
        self.handle.state.gateway_files.handle_file_response(file).await;
        Ok(())
      }
    }
  }
}
