use bluer::{Adapter, Session};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

#[cfg(debug_assertions)]
mod debug;

use crate::{protocol::Protocol, Callback, JsMessage, MsgRx, Result};

pub struct Rfcomm {
  session: Session,
  adapter: Adapter,

  rx: MsgRx,
  callbacks: Vec<Callback>,
  cancel_token: CancellationToken,
}

impl Rfcomm {
  async fn event_loop(&mut self) {
    if let Err(e) = self.initialize().await {
      tracing::error!("failed to initialize rfcomm?? {:?}", e);
    };

    tracing::debug!("starting rfcomm event loop");

    loop {
      tokio::select! {
        Some(msg) = self.rx.recv() => {
          if let Err(e) = self.handle_js_msg(msg).await {
            tracing::error!("failed to handle js message: {:?}", e);
          }
        }
        _ = self.cancel_token.cancelled() => {
          tracing::debug!("rfcomm event loop cancelled");
          break;
        }
      }
    }

    tracing::debug!("rfcomm event loop exited");
  }

  async fn handle_js_msg(&mut self, msg: JsMessage) -> Result<()> {
    tracing::debug!("received JS message: {msg:?}");

    Ok(())
  }

  async fn initialize(&mut self) -> Result<()> {
    tracing::debug!("initializing rfcomm protocol");

    Ok(())
  }
}

#[async_trait::async_trait]
impl Protocol for Rfcomm {
  async fn init(
    adapter_name: Option<String>,
    rx: MsgRx,
    callbacks: Vec<Callback>,
    cancel_token: CancellationToken,
  ) -> Result<Self> {
    tracing::debug!("initializing bluetooth adapter");
    let session = Session::new().await?;
    let adapter = if let Some(adapter_name) = adapter_name {
      session.adapter(&adapter_name)
    } else {
      session.default_adapter().await
    }?;

    tracing::debug!("attempting to power on adapter");
    adapter.set_powered(true).await?;

    #[cfg(debug_assertions)]
    debug::query_adapter(&adapter).await?;

    tracing::debug!("initialized bluetooth adapter {}", adapter.name());

    Ok(Self {
      session,
      adapter,

      rx,
      callbacks,
      cancel_token,
    })
  }

  fn spawn(mut self) -> JoinHandle<()> {
    tokio::spawn(async move { self.event_loop().await })
  }
}
