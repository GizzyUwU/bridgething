use std::collections::HashMap;

use tokio::net::TcpListener;

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

#[cfg(not(feature = "systemd"))]
pub fn inherited_listeners() -> HashMap<String, TcpListener> {
  HashMap::new()
}
