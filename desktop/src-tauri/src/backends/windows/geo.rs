use std::sync::{Arc, mpsc::Receiver};

use bridgething_companion::backend::{GeoAccuracy, GeoError, Position};
use windows::{
  Devices::Geolocation::{
    AltitudeReferenceSystem, GeolocationAccessStatus, Geolocator, Geoposition, PositionAccuracy,
    PositionChangedEventArgs, PositionStatus, StatusChangedEventArgs,
  },
  Foundation::TypedEventHandler,
  Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize},
  core::HRESULT,
};

use crate::backends::geo::{Command, Shared};

const ACCESS_DENIED: HRESULT = HRESULT(0x8007_0005_u32 as i32);
const TICKS_PER_SECOND: i64 = 10_000_000;
const EPOCH_OFFSET_SECONDS: i64 = 11_644_473_600;

pub fn run(shared: Arc<Shared>, commands: Receiver<Command>) {
  // SAFETY: winrt callbacks land on threadpool threads, so this engine owns an mta of its own.
  let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
  match Geolocator::new() {
    Ok(locator) => engine(&shared, &locator, commands),
    Err(error) => {
      tracing::warn!(%error, "windows refused a geolocator; this desktop cannot locate itself");
      shared.publish_authorization(false);
      shared.report(|inbox| inbox.on_error(GeoError::Unavailable));
    }
  }
  shared.park();
  unsafe { CoUninitialize() };
}

fn engine(shared: &Arc<Shared>, locator: &Geolocator, commands: Receiver<Command>) {
  let status = watch_status(shared, locator);
  let mut positions = None;

  while let Ok(command) = commands.recv() {
    match command {
      Command::Configure(accuracy) => {
        let wanted = match accuracy {
          GeoAccuracy::Coarse => PositionAccuracy::Default,
          GeoAccuracy::Fine => PositionAccuracy::High,
        };
        if let Err(error) = locator.SetDesiredAccuracy(wanted) {
          tracing::warn!(%error, "the geolocator kept its own accuracy");
        }
      }
      Command::RequestAuthorization => authorize(shared),
      Command::StartUpdating => {
        shared.set_watching(true);
        positions = positions.or_else(|| watch_positions(shared, locator));
      }
      Command::StopUpdating => {
        shared.set_watching(false);
        release(locator, positions.take());
      }
      Command::RequestOnce => {
        shared.set_one_shot(true);
        once(shared, locator);
      }
      Command::CancelOnce => {
        shared.take_one_shot();
      }
      Command::Shutdown => break,
    }
  }

  release(locator, positions);
  if let Some(token) = status {
    let _ = locator.RemoveStatusChanged(token);
  }
}

fn authorize(shared: &Shared) {
  match Geolocator::RequestAccessAsync().and_then(|access| access.join()) {
    Ok(status) => shared.publish_authorization(status == GeolocationAccessStatus::Allowed),
    Err(error) => {
      tracing::warn!(%error, "windows would not say whether location is allowed");
      shared.publish_authorization(false);
    }
  }
}

fn once(shared: &Arc<Shared>, locator: &Geolocator) {
  let asked = locator.GetGeopositionAsync().and_then(|pending| pending.join());
  shared.set_one_shot(false);
  match asked {
    Ok(position) => deliver(shared, &position),
    Err(error) => {
      let mapped = if error.code() == ACCESS_DENIED {
        GeoError::PermissionDenied
      } else {
        GeoError::Unavailable
      };
      tracing::warn!(%error, ?mapped, "the geolocator refused a fix");
      shared.report(|inbox| inbox.on_error(mapped));
    }
  }
}

fn watch_positions(shared: &Arc<Shared>, locator: &Geolocator) -> Option<i64> {
  let held = Arc::clone(shared);
  let handler = TypedEventHandler::<Geolocator, PositionChangedEventArgs>::new(move |_, args| {
    let Ok(args) = args.ok() else { return Ok(()) };
    let Ok(position) = args.Position() else { return Ok(()) };
    held.set_one_shot(false);
    deliver(&held, &position);
    Ok(())
  });
  match locator.PositionChanged(&handler) {
    Ok(token) => Some(token),
    Err(error) => {
      tracing::warn!(%error, "the geolocator refused a position subscription");
      None
    }
  }
}

fn watch_status(shared: &Arc<Shared>, locator: &Geolocator) -> Option<i64> {
  let held = Arc::clone(shared);
  let handler = TypedEventHandler::<Geolocator, StatusChangedEventArgs>::new(move |_, args| {
    let Ok(args) = args.ok() else { return Ok(()) };
    match args.Status() {
      Ok(PositionStatus::Ready) => held.publish_authorization(true),
      Ok(PositionStatus::Disabled) => {
        held.publish_authorization(false);
        held.report(|inbox| inbox.on_error(GeoError::PermissionDenied));
      }
      Ok(PositionStatus::NotAvailable) => held.report(|inbox| inbox.on_error(GeoError::Unavailable)),
      _ => {}
    }
    Ok(())
  });
  match locator.StatusChanged(&handler) {
    Ok(token) => Some(token),
    Err(error) => {
      tracing::warn!(%error, "the geolocator refused a status subscription");
      None
    }
  }
}

fn release(locator: &Geolocator, token: Option<i64>) {
  if let Some(token) = token {
    let _ = locator.RemovePositionChanged(token);
  }
}

fn deliver(shared: &Shared, position: &Geoposition) {
  match fix(position) {
    Some(fix) => shared.report(|inbox| inbox.on_position(fix)),
    None => {
      tracing::warn!("the geolocator published a position that could not be read");
      shared.report(|inbox| inbox.on_error(GeoError::Unavailable));
    }
  }
}

fn fix(position: &Geoposition) -> Option<Position> {
  let coordinate = position.Coordinate().ok()?;
  let point = coordinate.Point().ok()?;
  let basic = point.Position().ok()?;
  let known_altitude = point.AltitudeReferenceSystem().ok()? != AltitudeReferenceSystem::Unspecified;
  let ticks = coordinate.Timestamp().ok()?.UniversalTime;

  Some(Position {
    lat: basic.Latitude,
    lon: basic.Longitude,
    alt_m: known_altitude.then_some(basic.Altitude as f32),
    accuracy_m: coordinate.Accuracy().unwrap_or_default().max(0.0) as f32,
    speed_mps: coordinate
      .Speed()
      .and_then(|speed| speed.Value())
      .ok()
      .map(|speed| speed as f32),
    heading_deg: coordinate
      .Heading()
      .and_then(|heading| heading.Value())
      .ok()
      .map(|heading| heading as f32),
    ts_unix_s: (ticks / TICKS_PER_SECOND - EPOCH_OFFSET_SECONDS).clamp(0, i64::from(u32::MAX)) as u32,
  })
}
