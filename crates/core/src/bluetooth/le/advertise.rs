//! LE advertisement registration for the pair-trigger service. Written
//! against zbus rather than bluer's `Adapter::advertise` because bluer
//! 0.17 doesn't expose every BlueZ property we want to pin (LocalName +
//! Includes + tx-power live on different paths in the wrapper).
//!
//! Carries `ServiceUUIDs = [PAIR_TRIGGER_SERVICE]` so the iOS companion
//! app's `AccessorySetupKit` picker (and CoreBluetooth's
//! `scanForPeripherals(withServices:)` filter) can find us — both match
//! against advertised service UUIDs, not the LE Service-Solicitation AD
//! type. The legacy 31-byte ADV PDU can't fit a second 128-bit UUID
//! alongside Flags + LocalName + tx-power; we drop ANCS solicit
//! deliberately. ANCS is exposed by iOS to any LE-bonded peer that
//! holds the per-bond authorization, regardless of solicit history;
//! the app issues `connect(_, options: [CBConnectPeripheralOptionRequiresANCS: true])`
//! to drive iOS to surface the ANCS authorization prompt after pair.

use std::collections::HashMap;

use uuid::Uuid;
use zbus::{
  Connection,
  zvariant::{ObjectPath, OwnedObjectPath, Value},
};

const ADV_OBJECT_PATH: &str = "/dev/bridgething/le/adv0";
const ADV_LOCAL_NAME: &str = "Bridgething";

const ADV_MIN_INTERVAL_MS: u32 = 100;
const ADV_MAX_INTERVAL_MS: u32 = 150;

#[zbus::proxy(interface = "org.bluez.LEAdvertisingManager1", default_service = "org.bluez")]
trait LEAdvertisingManager {
  fn register_advertisement(
    &self,
    advertisement: &ObjectPath<'_>,
    options: HashMap<&str, Value<'_>>,
  ) -> zbus::Result<()>;

  fn unregister_advertisement(&self, advertisement: &ObjectPath<'_>) -> zbus::Result<()>;
}

struct LeAdvertisementImpl {
  service_uuids: Vec<String>,
}

#[zbus::interface(name = "org.bluez.LEAdvertisement1")]
impl LeAdvertisementImpl {
  #[zbus(property, name = "Type")]
  fn r#type(&self) -> &str {
    "peripheral"
  }

  #[zbus(property, name = "ServiceUUIDs")]
  fn service_uuids(&self) -> Vec<String> {
    self.service_uuids.clone()
  }

  #[zbus(property, name = "Discoverable")]
  fn discoverable(&self) -> bool {
    true
  }

  #[zbus(property, name = "LocalName")]
  fn local_name(&self) -> &str {
    ADV_LOCAL_NAME
  }

  #[zbus(property, name = "Includes")]
  fn includes(&self) -> Vec<String> {
    vec!["tx-power".to_string()]
  }

  #[zbus(property, name = "MinInterval")]
  fn min_interval(&self) -> u32 {
    ADV_MIN_INTERVAL_MS
  }

  #[zbus(property, name = "MaxInterval")]
  fn max_interval(&self) -> u32 {
    ADV_MAX_INTERVAL_MS
  }

  fn release(&self) {
    tracing::debug!("BlueZ released LE advertisement");
  }
}

pub struct LeAdvertisement {
  conn: Connection,
  path: OwnedObjectPath,
  adapter_path: OwnedObjectPath,
}

impl LeAdvertisement {
  pub async fn register(adapter_dbus_path: &str, advertised_service_uuid: Uuid) -> Result<Self, AdvertiseError> {
    let conn = Connection::system().await?;
    let path: OwnedObjectPath = ObjectPath::try_from(ADV_OBJECT_PATH)?.into();

    conn
      .object_server()
      .at(
        &path,
        LeAdvertisementImpl {
          service_uuids: vec![advertised_service_uuid.to_string()],
        },
      )
      .await?;

    let adapter_path: OwnedObjectPath = ObjectPath::try_from(adapter_dbus_path)?.into();
    let proxy = LEAdvertisingManagerProxy::builder(&conn)
      .destination("org.bluez")?
      .path(&adapter_path)?
      .build()
      .await?;

    proxy.register_advertisement(&path.as_ref(), HashMap::new()).await?;

    Ok(Self {
      conn,
      path,
      adapter_path,
    })
  }
}

impl Drop for LeAdvertisement {
  fn drop(&mut self) {
    let conn = self.conn.clone();
    let path = self.path.clone();
    let adapter_path = self.adapter_path.clone();
    tokio::spawn(async move {
      if let Ok(proxy) = LEAdvertisingManagerProxy::builder(&conn)
        .destination("org.bluez")
        .and_then(|b| b.path(&adapter_path))
        .map(|b| b.build())
        && let Ok(p) = proxy.await
        && let Err(err) = p.unregister_advertisement(&path.as_ref()).await
      {
        tracing::trace!(
          ?err,
          "LE advertisement unregister failed (BlueZ may already have released it)"
        );
      }
      let _ = conn.object_server().remove::<LeAdvertisementImpl, _>(&path).await;
    });
  }
}

#[derive(Debug, thiserror::Error)]
pub enum AdvertiseError {
  #[error(transparent)]
  Zbus(#[from] zbus::Error),
  #[error(transparent)]
  ZvariantPath(#[from] zbus::zvariant::Error),
}
