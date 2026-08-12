use std::{
  sync::{
    Arc,
    mpsc::{Receiver, TryRecvError},
  },
  thread,
  time::Duration,
};

use bridgething_companion::backend::{GeoAccuracy, GeoError, Position};
use objc2::{AnyThread, DefinedClass, define_class, msg_send, rc::Retained, runtime::ProtocolObject};
use objc2_core_foundation::{CFRunLoop, CFRunLoopRunResult, kCFRunLoopDefaultMode};
use objc2_core_location::{
  CLAuthorizationStatus, CLError, CLLocation, CLLocationManager, CLLocationManagerDelegate, kCLLocationAccuracyBest,
  kCLLocationAccuracyHundredMeters,
};
use objc2_foundation::{NSArray, NSError, NSObject, NSObjectProtocol};

use crate::backends::geo::{Command, Shared};

const RUN_LOOP_SLICE: f64 = 0.25;
const IDLE_SLICE: Duration = Duration::from_millis(250);

pub fn run(shared: Arc<Shared>, commands: Receiver<Command>) {
  let delegate = LocationDelegate::new(Arc::clone(&shared));
  let manager = unsafe { CLLocationManager::new() };
  unsafe { manager.setDelegate(Some(ProtocolObject::from_ref(&*delegate))) };
  publish_authorization(&shared, &manager);

  'engine: loop {
    loop {
      match commands.try_recv() {
        Ok(Command::Shutdown) | Err(TryRecvError::Disconnected) => break 'engine,
        Ok(command) => apply(&manager, &shared, command),
        Err(TryRecvError::Empty) => break,
      }
    }
    if CFRunLoop::run_in_mode(unsafe { kCFRunLoopDefaultMode }, RUN_LOOP_SLICE, false) == CFRunLoopRunResult::Finished {
      thread::sleep(IDLE_SLICE);
    }
  }

  unsafe {
    manager.stopUpdatingLocation();
    manager.setDelegate(None);
  }
  shared.park();
}

fn apply(manager: &CLLocationManager, shared: &Shared, command: Command) {
  unsafe {
    match command {
      Command::Configure(accuracy) => manager.setDesiredAccuracy(match accuracy {
        GeoAccuracy::Coarse => kCLLocationAccuracyHundredMeters,
        GeoAccuracy::Fine => kCLLocationAccuracyBest,
      }),
      Command::RequestAuthorization => {
        if manager.authorizationStatus() == CLAuthorizationStatus::NotDetermined {
          manager.requestWhenInUseAuthorization();
        }
      }
      Command::StartUpdating => {
        shared.set_watching(true);
        manager.startUpdatingLocation();
      }
      Command::StopUpdating => {
        shared.set_watching(false);
        manager.stopUpdatingLocation();
      }
      Command::RequestOnce => {
        shared.set_one_shot(true);
        manager.requestLocation();
      }
      Command::CancelOnce => {
        if shared.take_one_shot() && !shared.watching() {
          manager.stopUpdatingLocation();
        }
      }
      Command::Shutdown => {}
    }
  }
}

fn publish_authorization(shared: &Shared, manager: &CLLocationManager) {
  shared.publish_authorization(grantable(unsafe { manager.authorizationStatus() }));
}

fn grantable(status: CLAuthorizationStatus) -> bool {
  !matches!(
    status,
    CLAuthorizationStatus::Denied | CLAuthorizationStatus::Restricted
  )
}

fn position(location: &CLLocation) -> Position {
  let coordinate = unsafe { location.coordinate() };
  let horizontal = unsafe { location.horizontalAccuracy() };
  let vertical = unsafe { location.verticalAccuracy() };
  let speed = unsafe { location.speed() };
  let course = unsafe { location.course() };
  let since_epoch = unsafe { location.timestamp() }.timeIntervalSince1970();

  Position {
    lat: coordinate.latitude,
    lon: coordinate.longitude,
    alt_m: (vertical >= 0.0).then(|| unsafe { location.altitude() } as f32),
    accuracy_m: if horizontal.is_finite() {
      horizontal.max(0.0) as f32
    } else {
      0.0
    },
    speed_mps: (speed >= 0.0).then_some(speed as f32),
    heading_deg: (course >= 0.0).then_some(course as f32),
    ts_unix_s: since_epoch.max(0.0) as u32,
  }
}

struct DelegateState {
  shared: Arc<Shared>,
}

define_class!(
  // SAFETY: NSObject has no subclassing requirements and this class has no Drop.
  #[unsafe(super(NSObject))]
  #[ivars = DelegateState]
  struct LocationDelegate;

  unsafe impl NSObjectProtocol for LocationDelegate {}

  unsafe impl CLLocationManagerDelegate for LocationDelegate {
    #[unsafe(method(locationManager:didUpdateLocations:))]
    fn did_update_locations(&self, _manager: &CLLocationManager, locations: &NSArray<CLLocation>) {
      let Some(last) = locations.lastObject() else { return };
      let shared = &self.ivars().shared;
      shared.set_one_shot(false);
      let fix = position(&last);
      shared.report(|inbox| inbox.on_position(fix));
    }

    #[unsafe(method(locationManager:didFailWithError:))]
    fn did_fail(&self, manager: &CLLocationManager, error: &NSError) {
      let shared = &self.ivars().shared;
      shared.set_one_shot(false);
      let denied = error.code() == CLError::Denied.0 && !grantable(unsafe { manager.authorizationStatus() });
      let mapped = if denied {
        GeoError::PermissionDenied
      } else {
        GeoError::Unavailable
      };
      tracing::warn!(code = error.code(), ?mapped, "core location refused a fix");
      shared.report(|inbox| inbox.on_error(mapped));
    }

    #[unsafe(method(locationManagerDidChangeAuthorization:))]
    fn did_change_authorization(&self, manager: &CLLocationManager) {
      publish_authorization(&self.ivars().shared, manager);
    }
  }
);

impl LocationDelegate {
  fn new(shared: Arc<Shared>) -> Retained<Self> {
    let this = Self::alloc().set_ivars(DelegateState { shared });
    unsafe { msg_send![super(this), init] }
  }
}
