use libbridgething::gateway::{
  BridgeFile, BridgeToGatewayFileMsg, BridgeToGatewayMsgData, FileAdd, FileDelete, FileList, FileRequestData, FileResponseData,
  GatewayToBridgeFileMsg,
};

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
      GatewayToBridgeFileMsg::List => self.list().await,
      GatewayToBridgeFileMsg::Add(FileAdd { files }) => self.add(files).await,
      GatewayToBridgeFileMsg::Delete(FileDelete { files }) => self.delete(files).await,
      GatewayToBridgeFileMsg::FileResponse(FileResponseData { file }) => self.handle_file_response(file).await,
    }
  }

  async fn list(&self) -> HandlerResult {
    let files = self.handle.state.fs.list_files().await?;
    self.handle.respond(BridgeToGatewayFileMsg::Files(FileList { files })).await;

    Ok(())
  }

  async fn add(&self, files: Vec<BridgeFile>) -> HandlerResult {
    tracing::debug!("({:?}) adding files", &self.handle.address);

    let futures = files
      .into_iter()
      .map(|file| self.handle.state.fs.save_file(file.path, file.data))
      .collect::<Vec<_>>();

    for result in futures::future::join_all(futures).await {
      result?;
    }

    self.handle.respond(BridgeToGatewayMsgData::Done).await;
    Ok(())
  }

  async fn delete(&self, files: Vec<String>) -> HandlerResult {
    tracing::debug!("({:?}) deleting files", &self.handle.address);

    let futures = files
      .into_iter()
      .map(|file| self.handle.state.fs.delete_file(file))
      .collect::<Vec<_>>();

    for result in futures::future::join_all(futures).await {
      result?;
    }

    self.handle.respond(BridgeToGatewayMsgData::Done).await;
    Ok(())
  }

  async fn handle_file_response(&self, file: BridgeFile) -> HandlerResult {
    tracing::debug!("({:?}) handling file response", &self.handle.address);
    self.handle.state.fs.handle_file_response(file).await;
    Ok(())
  }
}
