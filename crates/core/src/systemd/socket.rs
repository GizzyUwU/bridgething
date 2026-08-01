use std::{
  collections::HashMap,
  os::fd::OwnedFd,
  sync::{Mutex, OnceLock},
};

use tokio::net::TcpListener;

pub const IAP2_FD_NAME: &str = "iap2-rfcomm";
static IAP2_FDS: OnceLock<Mutex<Vec<OwnedFd>>> = OnceLock::new();

fn park_iap2(fd: OwnedFd) {
  IAP2_FDS
    .get_or_init(|| Mutex::new(Vec::new()))
    .lock()
    .expect("iap2 fd park poisoned")
    .push(fd);
}

pub fn claim_inherited_iap2() -> Vec<OwnedFd> {
  match IAP2_FDS.get() {
    Some(fds) => std::mem::take(&mut *fds.lock().expect("iap2 fd park poisoned")),
    None => Vec::new(),
  }
}

#[cfg(feature = "systemd")]
pub fn inherited_listeners() -> HashMap<String, TcpListener> {
  use std::os::fd::FromRawFd;

  let mut out = HashMap::new();

  let fds = match sd_notify::listen_fds_with_names() {
    Ok(fds) => fds,
    Err(err) => {
      tracing::warn!("failed to read inherited sockets: {err:?}");
      return out;
    }
  };

  for (fd, name) in fds {
    if name == IAP2_FD_NAME {
      tracing::info!("adopted iAP2 RFCOMM socket carried across the restart");
      park_iap2(unsafe { OwnedFd::from_raw_fd(fd) });
      continue;
    }

    let listener = unsafe { std::net::TcpListener::from_raw_fd(fd) };
    if let Err(err) = listener.set_nonblocking(true) {
      tracing::error!("inherited socket {name:?} could not be made non-blocking: {err:?}");
      continue;
    }
    match TcpListener::from_std(listener) {
      Ok(listener) => {
        out.insert(name, listener);
      }
      Err(err) => tracing::error!("inherited socket {name:?} could not be adopted: {err:?}"),
    }
  }

  out
}

#[cfg(feature = "systemd")]
pub fn stash_iap2_fd(fd: std::os::fd::BorrowedFd<'_>) -> bool {
  use sd_notify::NotifyState;

  match sd_notify::notify_with_fds(&[NotifyState::FdStore, NotifyState::FdName(IAP2_FD_NAME)], &[fd]) {
    Ok(()) => true,
    Err(err) => {
      tracing::warn!(?err, "service manager refused the iAP2 RFCOMM socket");
      false
    }
  }
}

#[cfg(feature = "systemd")]
pub fn clear_iap2_fds() {
  use sd_notify::NotifyState;

  if let Err(err) = sd_notify::notify(&[NotifyState::FdStoreRemove, NotifyState::FdName(IAP2_FD_NAME)]) {
    tracing::debug!(?err, "could not clear the stored iAP2 RFCOMM socket");
  }
}

#[cfg(not(feature = "systemd"))]
pub fn inherited_listeners() -> HashMap<String, TcpListener> {
  HashMap::new()
}

#[cfg(not(feature = "systemd"))]
pub fn stash_iap2_fd(_fd: std::os::fd::BorrowedFd<'_>) -> bool {
  false
}

#[cfg(not(feature = "systemd"))]
pub fn clear_iap2_fds() {}
