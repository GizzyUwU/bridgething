use std::{
  ffi::c_int,
  ptr::NonNull,
  sync::{Arc, Mutex},
};

use block2::{Block, RcBlock};
use bridgething_companion::backend::{ConnectivityInbox, ConnectivityMonitor};
use dispatch2::{DispatchQueue, DispatchRetained};
use objc2::{rc::Retained, runtime::AnyObject};

type NwObject = NonNull<AnyObject>;

const NW_PATH_STATUS_SATISFIED: c_int = 1;

#[link(name = "Network", kind = "framework")]
unsafe extern "C" {
  fn nw_path_monitor_create() -> *mut AnyObject;
  fn nw_path_monitor_set_queue(monitor: NwObject, queue: NonNull<DispatchQueue>);
  fn nw_path_monitor_set_update_handler(monitor: NwObject, handler: &Block<dyn Fn(NwObject)>);
  fn nw_path_monitor_start(monitor: NwObject);
  fn nw_path_monitor_cancel(monitor: NwObject);
  fn nw_path_get_status(path: NwObject) -> c_int;
}

struct Watch {
  monitor: Retained<AnyObject>,
  _handler: RcBlock<dyn Fn(NwObject)>,
  _queue: DispatchRetained<DispatchQueue>,
}

// SAFETY: nw_path_monitor is an os_object, which apple documents as usable from any thread
unsafe impl Send for Watch {}

impl Drop for Watch {
  fn drop(&mut self) {
    unsafe { nw_path_monitor_cancel(NonNull::from(&*self.monitor)) };
  }
}

#[derive(Default)]
pub struct NwPathConnectivity {
  held: Mutex<Option<Watch>>,
}

impl ConnectivityMonitor for NwPathConnectivity {
  fn start(&self, inbox: Arc<ConnectivityInbox>) {
    self.stop();

    // SAFETY: nw_path_monitor_create hands back an owned reference
    let Some(monitor) = (unsafe { Retained::from_raw(nw_path_monitor_create()) }) else {
      tracing::warn!("Network.framework refused a path monitor; connectivity edges are not observed");
      return;
    };
    let handler = RcBlock::new(move |path: NwObject| {
      inbox.on_changed(unsafe { nw_path_get_status(path) } == NW_PATH_STATUS_SATISFIED);
    });
    let queue = DispatchQueue::new("com.bridgething.desktop.connectivity", None);

    let handle = NonNull::from(&*monitor);
    unsafe {
      nw_path_monitor_set_update_handler(handle, &handler);
      nw_path_monitor_set_queue(handle, NonNull::from(&*queue));
      nw_path_monitor_start(handle);
    }

    *self.held.lock().unwrap() = Some(Watch {
      monitor,
      _handler: handler,
      _queue: queue,
    });
  }

  fn stop(&self) {
    self.held.lock().unwrap().take();
  }
}
