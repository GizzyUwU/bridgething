//! LE advertisement registration for ANCS solicitation. Written
//! against zbus rather than going through bluer's `Adapter::advertise`
//! because bluer 0.17 doesn't expose every BlueZ property we want to
//! pin (LocalName + Includes + tx-power live on different paths in
//! the wrapper).
//!
//! Carries SolicitUUIDs = [ANCS] so iOS knows this peripheral wants
//! ANCS exposed to it once an LE bond exists, plus LocalName + Flags
//! + tx-power Includes per accessory guidelines. Total adv bytes are
//!   near the 31-byte legacy limit; adding additional UUIDs here
//!   overflows even after BlueZ's automatic SCAN_RSP split. The
//!   companion-app filter is "soliciting ANCS AND name starts with
//!   Bridgething."

use std::collections::HashMap;

use uuid::Uuid;
use zbus::{
  Connection,
  zvariant::{ObjectPath, OwnedObjectPath, Value},
};

const ADV_OBJECT_PATH: &str = "/dev/bridgething/ancs/adv0";
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

struct AncsLeAdvertisement {
  solicit_uuids: Vec<String>,
}

#[zbus::interface(name = "org.bluez.LEAdvertisement1")]
impl AncsLeAdvertisement {
  #[zbus(property, name = "Type")]
  fn r#type(&self) -> &str {
    "peripheral"
  }

  #[zbus(property, name = "SolicitUUIDs")]
  fn solicit_uuids(&self) -> Vec<String> {
    self.solicit_uuids.clone()
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
    tracing::debug!("BlueZ released ANCS LE advertisement");
  }
}

pub struct AncsAdvertisement {
  conn: Connection,
  path: OwnedObjectPath,
  adapter_path: OwnedObjectPath,
}

impl AncsAdvertisement {
  pub async fn register(adapter_dbus_path: &str, ancs_service_uuid: Uuid) -> Result<Self, AdvertiseError> {
    let conn = Connection::system().await?;
    let path: OwnedObjectPath = ObjectPath::try_from(ADV_OBJECT_PATH)?.into();

    conn
      .object_server()
      .at(
        &path,
        AncsLeAdvertisement {
          solicit_uuids: vec![ancs_service_uuid.to_string()],
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

impl Drop for AncsAdvertisement {
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
          "ANCS LE advertisement unregister failed (BlueZ may already have released it)"
        );
      }
      let _ = conn.object_server().remove::<AncsLeAdvertisement, _>(&path).await;
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
