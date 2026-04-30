use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::{Callbacks, MsgRx, Result, protocol::Protocol};

pub struct Rfcomm;

impl Rfcomm {
  pub async fn init(
    _adapter_name: Option<String>,
    _rx: crate::MsgRx,
    _callbacks: crate::Callbacks,
    _cancel_token: tokio_util::sync::CancellationToken,
  ) -> crate::Result<Self> {
    panic!("Rfcomm is not finished on macOS")
  }
}

#[async_trait::async_trait]
impl Protocol for Rfcomm {
  async fn init(
    adapter_name: Option<String>,
    rx: MsgRx,
    callbacks: Callbacks,
    cancel_token: CancellationToken,
  ) -> Result<Self> {
    panic!("Rfcomm is not finished on macOS")
  }

  fn spawn(mut self) -> JoinHandle<()> {
    panic!("Rfcomm is not finished on macOS")
  }
}
