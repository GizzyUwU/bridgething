use std::{
  sync::{
    Arc,
    mpsc::{Receiver, TryRecvError},
  },
  time::Duration,
};

use bridgething_companion::backend::{GeoAccuracy, GeoError, Position};
use futures::StreamExt;
use zbus::{Connection, Message, Proxy, zvariant::OwnedObjectPath};

use crate::backends::geo::{Command, Shared};

const GEOCLUE_SERVICE: &str = "org.freedesktop.GeoClue2";
const MANAGER_PATH: &str = "/org/freedesktop/GeoClue2/Manager";
const MANAGER_INTERFACE: &str = "org.freedesktop.GeoClue2.Manager";
const CLIENT_INTERFACE: &str = "org.freedesktop.GeoClue2.Client";
const LOCATION_INTERFACE: &str = "org.freedesktop.GeoClue2.Location";
const DESKTOP_ID: &str = "com.bridgething.desktop";

const ACCURACY_NEIGHBORHOOD: u32 = 5;
const ACCURACY_EXACT: u32 = 8;
const UNKNOWN: f64 = -1.0;
const COMMAND_SLICE: Duration = Duration::from_millis(250);

pub fn run(shared: Arc<Shared>, commands: Receiver<Command>) {
  match tokio::runtime::Builder::new_current_thread().enable_all().build() {
    Ok(runtime) => runtime.block_on(engine(&shared, commands)),
    Err(error) => {
      tracing::warn!(%error, "the location engine has no runtime");
      shared.publish_authorization(false);
    }
  }
  shared.park();
}

async fn engine(shared: &Shared, commands: Receiver<Command>) {
  let connection = match Connection::system().await {
    Ok(connection) => connection,
    Err(error) => return unreachable_bus(shared, error),
  };
  let client = match client(&connection).await {
    Ok(client) => client,
    Err(error) => return unreachable_bus(shared, error),
  };
  let mut updates = match client.receive_signal("LocationUpdated").await {
    Ok(updates) => updates,
    Err(error) => return unreachable_bus(shared, error),
  };
  shared.publish_authorization(true);

  'engine: loop {
    loop {
      match commands.try_recv() {
        Ok(Command::Shutdown) | Err(TryRecvError::Disconnected) => break 'engine,
        Ok(command) => apply(&client, shared, command).await,
        Err(TryRecvError::Empty) => break,
      }
    }
    tokio::select! {
      update = updates.next() => {
        let Some(update) = update else { break 'engine };
        deliver(&connection, &client, shared, &update).await;
      }
      _ = tokio::time::sleep(COMMAND_SLICE) => {}
    }
  }

  stop(&client).await;
}

fn unreachable_bus(shared: &Shared, error: impl std::fmt::Display) {
  tracing::warn!(%error, "geoclue is not on the bus; this desktop cannot locate itself");
  shared.publish_authorization(false);
  shared.report(|inbox| inbox.on_error(GeoError::Unavailable));
}

async fn client(connection: &Connection) -> zbus::Result<Proxy<'static>> {
  let manager = Proxy::new(connection, GEOCLUE_SERVICE, MANAGER_PATH, MANAGER_INTERFACE).await?;
  let path: OwnedObjectPath = manager.call("GetClient", &()).await?;
  let client = Proxy::new(connection, GEOCLUE_SERVICE, path, CLIENT_INTERFACE).await?;
  client.set_property("DesktopId", DESKTOP_ID).await?;
  Ok(client)
}

async fn apply(client: &Proxy<'_>, shared: &Shared, command: Command) {
  match command {
    Command::Configure(accuracy) => {
      let level = match accuracy {
        GeoAccuracy::Coarse => ACCURACY_NEIGHBORHOOD,
        GeoAccuracy::Fine => ACCURACY_EXACT,
      };
      if let Err(error) = client.set_property("RequestedAccuracyLevel", level).await {
        tracing::warn!(%error, "geoclue kept its own accuracy level");
      }
    }
    Command::RequestAuthorization => {}
    Command::StartUpdating => {
      shared.set_watching(true);
      start(client, shared).await;
    }
    Command::StopUpdating => {
      shared.set_watching(false);
      stop(client).await;
    }
    Command::RequestOnce => {
      shared.set_one_shot(true);
      start(client, shared).await;
    }
    Command::CancelOnce => {
      if shared.take_one_shot() && !shared.watching() {
        stop(client).await;
      }
    }
    Command::Shutdown => {}
  }
}

async fn start(client: &Proxy<'_>, shared: &Shared) {
  let Err(error) = client.call_method("Start", &()).await else {
    return;
  };
  let denied = match &error {
    zbus::Error::MethodError(name, _, _) => name.contains("AccessDenied") || name.contains("NotAuthorized"),
    _ => false,
  };
  tracing::warn!(%error, denied, "geoclue refused to hand out locations");
  if denied {
    shared.publish_authorization(false);
  }
  shared.report(|inbox| {
    inbox.on_error(if denied {
      GeoError::PermissionDenied
    } else {
      GeoError::Unavailable
    })
  });
}

async fn stop(client: &Proxy<'_>) {
  if let Err(error) = client.call_method("Stop", &()).await {
    tracing::debug!(%error, "geoclue was already stopped");
  }
}

async fn deliver(connection: &Connection, client: &Proxy<'_>, shared: &Shared, update: &Message) {
  let body = update.body();
  let Ok((_, current)) = body.deserialize::<(OwnedObjectPath, OwnedObjectPath)>() else {
    tracing::warn!("geoclue announced a location update without a location");
    return;
  };

  match fix(connection, current).await {
    Ok(position) => shared.report(|inbox| inbox.on_position(position)),
    Err(error) => {
      tracing::warn!(%error, "geoclue published a location that could not be read");
      shared.report(|inbox| inbox.on_error(GeoError::Unavailable));
    }
  }

  if shared.take_one_shot() && !shared.watching() {
    stop(client).await;
  }
}

async fn fix(connection: &Connection, path: OwnedObjectPath) -> zbus::Result<Position> {
  let location = Proxy::new(connection, GEOCLUE_SERVICE, path, LOCATION_INTERFACE).await?;
  let altitude: f64 = location.get_property("Altitude").await.unwrap_or(f64::MIN);
  let speed: f64 = location.get_property("Speed").await.unwrap_or(UNKNOWN);
  let heading: f64 = location.get_property("Heading").await.unwrap_or(UNKNOWN);
  let (seconds, _): (u64, u64) = location.get_property("Timestamp").await.unwrap_or_default();

  Ok(Position {
    lat: location.get_property("Latitude").await?,
    lon: location.get_property("Longitude").await?,
    alt_m: (altitude > f64::MIN).then_some(altitude as f32),
    accuracy_m: location.get_property::<f64>("Accuracy").await?.max(0.0) as f32,
    speed_mps: (speed >= 0.0).then_some(speed as f32),
    heading_deg: (heading >= 0.0).then_some(heading as f32),
    ts_unix_s: seconds.min(u64::from(u32::MAX)) as u32,
  })
}
