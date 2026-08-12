use std::collections::HashMap;

use bluer::{Adapter, AdapterEvent, Address, DeviceEvent, DeviceProperty, Session};
use futures::{Stream, StreamExt};
use tokio::task::JoinHandle;

use super::{
  BluetoothError, BluetoothResult,
  profiles::{BluetoothConnectionEvent, ProfileMan},
};

pub struct AdapterEventStream {
  pub stream: Box<dyn Stream<Item = AdapterEvent> + Send + Unpin>,
  pub adapter: Adapter,
}

impl AdapterEventStream {
  pub fn spawn(self, profile: ProfileMan) -> JoinHandle<()> {
    tracing::debug!("spawning adapter event stream");
    tokio::spawn(async move { self.event_loop(profile).await })
  }

  async fn event_loop(mut self, profile: ProfileMan) {
    let mut device_watchers: HashMap<Address, JoinHandle<()>> = HashMap::new();

    while let Some(msg) = self.stream.next().await {
      match msg {
        AdapterEvent::DeviceAdded(addr) => {
          if let Some(prev) = device_watchers.remove(&addr) {
            prev.abort();
          }
          match spawn_device_watcher(&self.adapter, addr, profile.clone()).await {
            Ok(handle) => {
              device_watchers.insert(addr, handle);
            }
            Err(err) => {
              tracing::warn!(?err, %addr, "failed to spawn per-device event watcher");
            }
          }

          if let Err(err) = profile
            .handle_event(BluetoothConnectionEvent::DeviceAdded { mac: addr.into() })
            .await
          {
            tracing::error!("failed to handle DeviceAdded: {:?}", err);
          }
        }
        AdapterEvent::DeviceRemoved(addr) => {
          if let Some(handle) = device_watchers.remove(&addr) {
            handle.abort();
          }
          if let Err(err) = profile
            .handle_event(BluetoothConnectionEvent::DeviceRemoved { mac: addr.into() })
            .await
          {
            tracing::error!("failed to handle DeviceRemoved: {:?}", err);
          }
        }
        AdapterEvent::PropertyChanged(prop) => {
          if let Err(err) = profile
            .handle_event(BluetoothConnectionEvent::AdapterPropertyChanged(prop))
            .await
          {
            tracing::error!("failed to handle adapter property change: {:?}", err);
          }
        }
      }
    }

    for (_, handle) in device_watchers.drain() {
      handle.abort();
    }
  }
}

async fn spawn_device_watcher(
  adapter: &Adapter,
  addr: Address,
  profile: ProfileMan,
) -> BluetoothResult<JoinHandle<()>> {
  let device = adapter.device(addr)?;
  let stream = device.events().await?;
  let stream = Box::pin(stream);

  Ok(tokio::spawn(async move {
    let mut stream = stream;
    while let Some(event) = stream.next().await {
      let DeviceEvent::PropertyChanged(prop) = event;
      let translated = match prop {
        DeviceProperty::Paired(paired) => Some(BluetoothConnectionEvent::PairedChanged {
          mac: addr.into(),
          paired,
        }),
        DeviceProperty::Connected(connected) => Some(BluetoothConnectionEvent::ConnectedChanged {
          mac: addr.into(),
          connected,
        }),
        _ => None,
      };
      if let Some(event) = translated
        && let Err(err) = profile.handle_event(event).await
      {
        tracing::error!(%addr, "failed to handle device property change: {:?}", err);
      }
    }
    tracing::trace!(%addr, "device event stream ended");
  }))
}

pub async fn get_adapter(session: &Session) -> BluetoothResult<Adapter> {
  let timeout = std::time::Duration::new(10, 0);
  let start = std::time::Instant::now();

  let adapter = loop {
    match session.default_adapter().await {
      Ok(adapter) => break adapter,
      Err(e) => {
        if start.elapsed() >= timeout {
          tracing::error!("Error getting default adapter - timing out: {:?}", e);
          return Err(BluetoothError::Timeout);
        }
        tracing::warn!("Error getting default adapter: {:?}", e);
        tokio::time::sleep(std::time::Duration::from_millis(750)).await;
        continue;
      }
    }
  };

  Ok(adapter)
}
