//! libswupdate FFI driver. Streams the .swu bytes over libswupdate's
//! install IPC socket (`ipc_inst_start_ext` / `ipc_send_data` /
//! `ipc_end`) and forwards progress messages from the separate
//! progress IPC socket (`progress_ipc_connect` / `progress_ipc_receive`)
//! into the OTA orchestrator's progress callback.
//!
//! All libswupdate calls are blocking syscalls against unix sockets
//! and live in `spawn_blocking` tasks so the tokio runtime stays free.
//! Cancellation post-stream-start is best-effort: closing the install
//! fd causes swupdate to abort the in-flight install on the daemon
//! side; mid-FAILURE the orchestrator surfaces `Cancelled` to the
//! caller and the failed slot retains its prior contents.

use std::os::raw::{c_char, c_int, c_uint, c_void};

use bridgething_swupdate_sys as sys;
use libbridgething::gateway::OtaPhase;
use tokio::{
  sync::{mpsc, watch},
  task,
};

use super::Error;

/// Bytes per chunk handed to `ipc_send_data`. libswupdate streams
/// these straight to its installer subprocess; smaller chunks give
/// the install side more frequent progress ticks at the cost of
/// IPC overhead.
const CHUNK_SIZE: usize = 64 * 1024;

pub async fn install_swu<F>(bytes: &[u8], progress: &F, cancel_rx: &mut watch::Receiver<bool>) -> Result<(), Error>
where
  F: Fn(OtaPhase, u8, Option<u32>) + Send + Sync,
{
  let (prog_tx, mut prog_rx) = mpsc::channel::<sys::progress_msg>(32);
  let _progress_handle = task::spawn_blocking(move || progress_reader(prog_tx));

  let owned = bytes.to_vec();
  let send_handle = task::spawn_blocking(move || install_blocking(owned));

  let mut send_handle = Some(send_handle);
  let mut send_done = false;

  loop {
    tokio::select! {
      Some(msg) = prog_rx.recv() => {
        let (phase, percent, eta) = translate(&msg);
        progress(phase, percent, eta);
        match msg.status {
          sys::RECOVERY_STATUS_SUCCESS => {
            tracing::info!("libswupdate reported SUCCESS");
            return Ok(());
          }
          sys::RECOVERY_STATUS_FAILURE => {
            let detail = info_str(&msg);
            tracing::warn!(detail = %detail, "libswupdate reported FAILURE");
            return Err(Error::InstallFailed(detail));
          }
          _ => {}
        }
      }
      res = wait_send(&mut send_handle), if send_handle.is_some() => {
        send_done = true;
        match res {
          Ok(Ok(())) => tracing::debug!("install bytes streamed; awaiting libswupdate completion via progress socket"),
          Ok(Err(e)) => return Err(e),
          Err(e) => return Err(Error::Ipc(format!("install task panic: {e}"))),
        }
      }
      _ = cancel_rx.changed() => {
        if *cancel_rx.borrow() {
          tracing::info!("ota cancellation observed; aborting install");
          return Err(Error::Cancelled);
        }
      }
      else => {
        // No more progress messages and the install task already finished.
        // libswupdate sometimes closes the progress socket without a final
        // SUCCESS in the message stream we picked up; treat as success only
        // if the send side completed cleanly.
        if send_done {
          tracing::warn!("progress socket closed after install bytes streamed; assuming success");
          return Ok(());
        }
        return Err(Error::Ipc("progress socket closed before install completed".into()));
      }
    }
  }
}

async fn wait_send(
  handle: &mut Option<task::JoinHandle<Result<(), Error>>>,
) -> Result<Result<(), Error>, task::JoinError> {
  let h = handle.take().expect("wait_send only called when handle is Some");
  h.await
}

fn install_blocking(bytes: Vec<u8>) -> Result<(), Error> {
  // SAFETY: every libswupdate IPC call below is documented blocking-but-safe;
  // `req` lives on this stack frame for the entire `ipc_inst_start_ext`
  // call and is not retained by the library beyond that.
  unsafe {
    let mut req: sys::swupdate_request = std::mem::zeroed();
    sys::swupdate_prepare_req(&mut req);
    req.apiversion = sys::SWUPDATE_API_VERSION;
    req.source = sys::sourcetype_SOURCE_LOCAL;
    req.dry_run = sys::run_type_RUN_INSTALL;

    let fd = sys::ipc_inst_start_ext(
      &mut req as *mut _ as *mut c_void,
      std::mem::size_of::<sys::swupdate_request>() as isize,
    );
    if fd < 0 {
      return Err(Error::Ipc(format!("ipc_inst_start_ext returned {fd}")));
    }

    let mut written = 0usize;
    while written < bytes.len() {
      let len = (bytes.len() - written).min(CHUNK_SIZE);
      let chunk_ptr = bytes.as_ptr().add(written) as *mut c_char;
      let r = sys::ipc_send_data(fd, chunk_ptr, len as c_int);
      if r < 0 {
        sys::ipc_end(fd);
        return Err(Error::Ipc(format!(
          "ipc_send_data returned {r} after {written}/{}",
          bytes.len()
        )));
      }
      written += len;
    }

    sys::ipc_end(fd);
  }
  Ok(())
}

fn progress_reader(tx: mpsc::Sender<sys::progress_msg>) {
  // SAFETY: `progress_ipc_connect` returns a unix-socket fd or a negative
  // error; `progress_ipc_receive` blocks until a message arrives or the
  // socket closes. `msg` is fully populated by libswupdate before return.
  unsafe {
    let mut fd = sys::progress_ipc_connect(true);
    if fd < 0 {
      tracing::warn!("progress_ipc_connect returned {fd}; no progress events will surface");
      return;
    }
    loop {
      let mut msg: sys::progress_msg = std::mem::zeroed();
      let r = sys::progress_ipc_receive(&mut fd, &mut msg);
      if r <= 0 {
        tracing::debug!("progress_ipc_receive returned {r}; reader exiting");
        return;
      }
      let terminal = matches!(msg.status, sys::RECOVERY_STATUS_SUCCESS | sys::RECOVERY_STATUS_FAILURE);
      if tx.blocking_send(msg).is_err() {
        return;
      }
      if terminal {
        return;
      }
    }
  }
}

fn translate(msg: &sys::progress_msg) -> (OtaPhase, u8, Option<u32>) {
  let phase = match msg.status {
    sys::RECOVERY_STATUS_DOWNLOAD => OtaPhase::Downloading,
    sys::RECOVERY_STATUS_DONE => OtaPhase::Confirming,
    sys::RECOVERY_STATUS_SUCCESS => OtaPhase::Reboot,
    // START / RUN / PROGRESS / SUBPROCESS / IDLE / FAILURE all map to Writing
    // for progress-display purposes; FAILURE is handled separately by the caller.
    _ => OtaPhase::Writing,
  };
  let percent = (msg.cur_percent.min(100)) as u8;
  (phase, percent, None)
}

fn info_str(msg: &sys::progress_msg) -> String {
  let bytes = &msg.info[..msg.infolen.min(msg.info.len() as c_uint) as usize];
  // libswupdate's info buffer is a NUL-terminated C string; trim at the
  // first 0 byte so we don't include trailing garbage from the fixed-size
  // array.
  let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
  String::from_utf8_lossy(&bytes[..end].iter().map(|&b| b as u8).collect::<Vec<u8>>()).into_owned()
}
