use tokio_util::sync::CancellationToken;
use warp::Filter;

const FILE_SERVE_PORT: u16 = 8891;

pub struct FileServe {
  cancel_token: CancellationToken,
  handle: tokio::task::JoinHandle<()>,
}

impl FileServe {
  pub fn init() -> Self {
    let cancel_token = CancellationToken::new();

    Self {
      cancel_token: cancel_token.clone(),
      handle: tokio::spawn(async {
        tracing::info!("starting file server on port {}", FILE_SERVE_PORT);
        file_server(cancel_token).await;
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

async fn file_server(cancel_token: CancellationToken) {
  let hello = warp::path!("hello" / String).map(|name| format!("Hello, {}!", name));

  tokio::select! {
    _ = warp::serve(hello).run(([0, 0, 0, 0], FILE_SERVE_PORT)) => {
      tracing::error!("file server stopped");
    }
    _ = cancel_token.cancelled() => {
      tracing::debug!("file server shutting down");
    }
  }
}
