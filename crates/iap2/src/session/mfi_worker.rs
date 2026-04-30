//! Dedicated worker that owns the MFi coprocessor handle.
//!
//! `MfiAuth::cert` and `MfiAuth::sign` are blocking I²C ops; sign in
//! particular sleeps ~500 ms inside the chip's signing window. The
//! session task can't call them inline without stalling the runtime.
//! `WorkerMfiAccess` runs the `MfiAuth` on a `spawn_blocking` thread,
//! receives requests over an mpsc, replies via oneshot, and exposes
//! the same async surface as any other [`MfiAccess`] impl.

use std::thread::JoinHandle;

use async_trait::async_trait;
use bridgething_mfi::{CHALLENGE_LEN, Error as MfiError, MfiAuth, RESPONSE_LEN, Transport};
use bytes::Bytes;
use tokio::sync::{mpsc, oneshot};

use super::{MfiAccess, MfiResult};

enum MfiRequest {
  Cert(oneshot::Sender<Result<Bytes, MfiError>>),
  Sign {
    challenge: [u8; CHALLENGE_LEN],
    reply: oneshot::Sender<Result<[u8; RESPONSE_LEN], MfiError>>,
  },
}

/// `MfiAccess` impl that drives an `MfiAuth<T>` on a dedicated thread.
/// Construct with [`WorkerMfiAccess::spawn`]; drop to shut the worker
/// down (the receive loop exits when the request channel closes).
pub struct WorkerMfiAccess {
  tx: mpsc::Sender<MfiRequest>,
  _join: JoinHandle<()>,
}

impl WorkerMfiAccess {
  pub fn spawn<T>(mfi: MfiAuth<T>) -> Self
  where
    T: Transport + Send + 'static,
  {
    let (tx, rx) = mpsc::channel(8);
    let join = std::thread::Builder::new()
      .name("iap2-mfi-worker".into())
      .spawn(move || worker_loop(mfi, rx))
      .expect("spawn iap2-mfi-worker thread");
    Self { tx, _join: join }
  }
}

#[async_trait]
impl MfiAccess for WorkerMfiAccess {
  async fn cert(&mut self) -> MfiResult<Bytes> {
    let (reply_tx, reply_rx) = oneshot::channel();
    self
      .tx
      .send(MfiRequest::Cert(reply_tx))
      .await
      .map_err(|_| MfiError::Transport(bridgething_mfi::TransportError::Other("mfi worker gone".into())))?;
    reply_rx.await.map_err(|_| {
      MfiError::Transport(bridgething_mfi::TransportError::Other(
        "mfi worker dropped reply".into(),
      ))
    })?
  }

  async fn sign(&mut self, challenge: [u8; CHALLENGE_LEN]) -> MfiResult<[u8; RESPONSE_LEN]> {
    let (reply_tx, reply_rx) = oneshot::channel();
    self
      .tx
      .send(MfiRequest::Sign {
        challenge,
        reply: reply_tx,
      })
      .await
      .map_err(|_| MfiError::Transport(bridgething_mfi::TransportError::Other("mfi worker gone".into())))?;
    reply_rx.await.map_err(|_| {
      MfiError::Transport(bridgething_mfi::TransportError::Other(
        "mfi worker dropped reply".into(),
      ))
    })?
  }
}

fn worker_loop<T: Transport>(mut mfi: MfiAuth<T>, mut rx: mpsc::Receiver<MfiRequest>) {
  while let Some(req) = rx.blocking_recv() {
    match req {
      MfiRequest::Cert(reply) => {
        let r = mfi.cert().map(Bytes::from);
        let _ = reply.send(r);
      }
      MfiRequest::Sign { challenge, reply } => {
        let r = mfi.sign(&challenge);
        let _ = reply.send(r);
      }
    }
  }
}
