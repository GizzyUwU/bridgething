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

use std::{
  ffi::CString,
  io::Read,
  os::raw::{c_char, c_int, c_uint, c_void},
  path::{Path, PathBuf},
  sync::Once,
  time::{Duration, Instant},
};

use bridgething_swupdate_sys as sys;
use libbridgething::OtaPhase;
use tokio::{
  sync::{mpsc, watch},
  task,
};

use super::{Error, ProgressTick, Selector};
const CHUNK_SIZE: usize = 64 * 1024;
const PROGRESS_MIN_INTERVAL: Duration = Duration::from_millis(100);
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(15);
const SWUPDATE_CTRL_SOCKET: &str = "/tmp/sockinstctrl";
const SWUPDATE_PROGRESS_SOCKET: &str = "/tmp/swupdateprog";

unsafe extern "C" {
  static mut SOCKET_CTRL_PATH: *mut c_char;
}

fn ensure_socket_paths() {
  static ONCE: Once = Once::new();
  ONCE.call_once(|| {
    let ctrl = CString::new(SWUPDATE_CTRL_SOCKET).expect("ctrl path has no NUL");
    let prog = CString::new(SWUPDATE_PROGRESS_SOCKET).expect("progress path has no NUL");
    unsafe {
      SOCKET_CTRL_PATH = ctrl.into_raw();
      sys::SOCKET_PROGRESS_PATH = prog.into_raw();
    }
  });
}

pub async fn install_swu<F>(
  swu_path: &Path,
  selector: &Selector,
  progress: &F,
  cancel_rx: &mut watch::Receiver<bool>,
) -> Result<(), Error>
where
  F: Fn(ProgressTick) + Send + Sync,
{
  ensure_socket_paths();
  let (prog_tx, mut prog_rx) = mpsc::channel::<sys::progress_msg>(32);
  let _progress_handle = task::spawn_blocking(move || progress_reader(prog_tx));

  let path = swu_path.to_path_buf();
  let selector = selector.clone();
  let send_handle = task::spawn_blocking(move || install_blocking(path, selector));

  let mut send_handle = Some(send_handle);
  let mut send_done = false;
  let mut last_emit: Option<(ProgressKey, Instant)> = None;
  let mut last_tick: Option<ProgressTick> = None;
  let mut heartbeat = tokio::time::interval(HEARTBEAT_INTERVAL);
  heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
  heartbeat.tick().await;

  loop {
    tokio::select! {
      Some(msg) = prog_rx.recv() => {
        let tick = translate(&msg);
        last_tick = Some(tick);
        heartbeat.reset();
        let terminal = matches!(msg.status, sys::RECOVERY_STATUS_SUCCESS | sys::RECOVERY_STATUS_FAILURE);
        if should_emit(&mut last_emit, &tick, terminal) {
          progress(tick);
        }
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
      _ = heartbeat.tick() => {
        let beat = last_tick.unwrap_or(ProgressTick {
          phase: OtaPhase::Writing,
          percent: 0,
          step: 0,
          nsteps: 0,
          dwl_percent: 0,
          dwl_bytes: 0,
          eta_ms: None,
        });
        progress(beat);
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

#[derive(Clone, Copy, PartialEq, Eq)]
struct ProgressKey {
  phase: OtaPhase,
  percent: u8,
  step: u8,
  dwl_percent: u8,
}

fn should_emit(last: &mut Option<(ProgressKey, Instant)>, tick: &ProgressTick, terminal: bool) -> bool {
  let now = Instant::now();
  let key = ProgressKey {
    phase: tick.phase,
    percent: tick.percent,
    step: tick.step,
    dwl_percent: tick.dwl_percent,
  };
  let (phase_changed, dup, intervaled) = match last {
    Some((prev, t)) => (
      prev.phase != key.phase,
      *prev == key,
      now.duration_since(*t) < PROGRESS_MIN_INTERVAL,
    ),
    None => (true, false, false),
  };
  if !terminal && !phase_changed && (dup || intervaled) {
    return false;
  }
  *last = Some((key, now));
  true
}

fn write_cstr_field(field: &mut [c_char], value: &str) -> Result<(), Error> {
  let bytes = value.as_bytes();
  if bytes.len() + 1 > field.len() {
    return Err(Error::Ipc(format!(
      "selector field {value:?} too long for swupdate_request slot ({} bytes)",
      field.len()
    )));
  }
  for (dst, src) in field.iter_mut().zip(bytes.iter().copied().chain(std::iter::repeat(0))) {
    *dst = src as c_char;
  }
  Ok(())
}

fn install_blocking(swu_path: PathBuf, selector: Selector) -> Result<(), Error> {
  let mut file = std::fs::File::open(&swu_path)?;
  let total_len = file.metadata()?.len();
  let mut buf = vec![0u8; CHUNK_SIZE];

  // SAFETY: every libswupdate IPC call below is documented blocking-but-safe;
  // `req` lives on this stack frame for the entire `ipc_inst_start_ext`
  // call and is not retained by the library beyond that.
  unsafe {
    let mut req: sys::swupdate_request = std::mem::zeroed();
    sys::swupdate_prepare_req(&mut req);
    req.apiversion = sys::SWUPDATE_API_VERSION;
    req.source = sys::sourcetype_SOURCE_LOCAL;
    req.dry_run = sys::run_type_RUN_INSTALL;
    write_cstr_field(&mut req.software_set, &selector.software_set)?;
    write_cstr_field(&mut req.running_mode, &selector.running_mode)?;

    let fd = sys::ipc_inst_start_ext(
      &mut req as *mut _ as *mut c_void,
      std::mem::size_of::<sys::swupdate_request>() as isize,
    );
    if fd < 0 {
      return Err(Error::Ipc(format!("ipc_inst_start_ext returned {fd}")));
    }

    let mut sent = 0u64;
    loop {
      let n = match file.read(&mut buf) {
        Ok(0) => break,
        Ok(n) => n,
        Err(err) => {
          sys::ipc_end(fd);
          return Err(Error::Io(err));
        }
      };
      let r = sys::ipc_send_data(fd, buf.as_ptr() as *mut c_char, n as c_int);
      if r < 0 {
        sys::ipc_end(fd);
        return Err(Error::Ipc(format!(
          "ipc_send_data returned {r} after {sent}/{total_len}"
        )));
      }
      sent += n as u64;
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

fn translate(msg: &sys::progress_msg) -> ProgressTick {
  ProgressTick {
    phase: OtaPhase::Writing,
    percent: msg.cur_percent.min(100) as u8,
    step: msg.cur_step.min(u8::MAX as c_uint) as u8,
    nsteps: msg.nsteps.min(u8::MAX as c_uint) as u8,
    dwl_percent: msg.dwl_percent.min(100) as u8,
    dwl_bytes: (msg.dwl_bytes as u64).min(u32::MAX as u64) as u32,
    eta_ms: None,
  }
}

fn info_str(msg: &sys::progress_msg) -> String {
  let bytes = &msg.info[..msg.infolen.min(msg.info.len() as c_uint) as usize];
  let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
  String::from_utf8_lossy(&bytes[..end].iter().map(|&b| b as u8).collect::<Vec<u8>>()).into_owned()
}
