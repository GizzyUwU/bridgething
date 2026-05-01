use std::{collections::HashMap, sync::Arc};

use libbridgething::gateway::BridgeFile;
use mime_guess::Mime;
use tokio::sync::Mutex;

pub type FileRequestTx = tokio::sync::oneshot::Sender<(Vec<u8>, Mime)>;

/// Tracks in-flight requests for assets that the gateway is expected to
/// supply on demand. The HTTP server, on a miss inside the `/_gateway/`
/// namespace, calls `request_file` and waits on the oneshot. When the
/// gateway responds with a `FileResponse`, `handle_file_response` resolves
/// every waiter pending on that path.
#[derive(Clone, Debug, Default)]
pub struct GatewayFileBridge {
  requests: Arc<Mutex<HashMap<String, Vec<FileRequestTx>>>>,
}

impl GatewayFileBridge {
  pub fn new() -> Self {
    Self::default()
  }

  pub async fn request_file(&self, path: String, tx: FileRequestTx) {
    tracing::debug!("queueing request for gateway-served file: {}", path);
    let mut requests = self.requests.lock().await;
    requests.entry(path).or_default().push(tx);
  }

  pub async fn handle_file_response(&self, file: BridgeFile) {
    tracing::debug!("handling file response from gateway for {}", file.path);
    let mime = mime_guess::from_path(&file.path).first_or_octet_stream();
    let mut requests = self.requests.lock().await;

    if let Some(tx_list) = requests.remove(&file.path) {
      for tx in tx_list {
        if let Err(e) = tx.send((file.data.clone(), mime.clone())) {
          tracing::error!("failed to send file response: {:?}", e);
        }
      }
    } else {
      tracing::warn!("no requests for file {}", file.path);
    }
  }
}
