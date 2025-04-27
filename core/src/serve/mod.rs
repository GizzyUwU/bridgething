use libbridgething::BRIDGETHING_FILE_SERVE_PORT;
use tokio_util::sync::CancellationToken;

use crate::state::State;

#[derive(Debug)]
pub struct FileServe {
  cancel_token: CancellationToken,
  handle: tokio::task::JoinHandle<()>,
}

impl FileServe {
  pub fn init(state: State) -> Self {
    let cancel_token = CancellationToken::new();

    Self {
      cancel_token: cancel_token.clone(),
      handle: tokio::spawn(async {
        tracing::info!("starting file server on port {}", BRIDGETHING_FILE_SERVE_PORT);
        file_server(state, cancel_token).await;
      }),
    }
  }

  pub async fn shutdown(self) {
    self.cancel_token.cancel();
    if let Err(err) = self.handle.await {
      tracing::error!("failed to shutdown file server: {:?}", err);
    }
  }
}

async fn file_server(state: State, cancel_token: CancellationToken) {
  loop {
    tokio::select! {
    _ = warp::serve(warp::fs::dir(state.fs.root.clone())).bind(([0, 0, 0, 0], BRIDGETHING_FILE_SERVE_PORT)) => {
        tracing::error!("file server stopped");
      }
      _ = cancel_token.cancelled() => {
        tracing::debug!("file server shutting down");
        break;
      }
     }
  }
}
