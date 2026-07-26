use std::{collections::HashMap, sync::OnceLock};

use uuid::Uuid;
use zbus::{
  Connection,
  zvariant::{ObjectPath, OwnedObjectPath, Value},
};

const ADV_OBJECT_PATH: &str = "/dev/bridgething/le/adv0";
const ADV_LOCAL_NAME_PREFIX: &str = "Car Thing (SN: ";
static ADV_LOCAL_NAME: OnceLock<&'static str> = OnceLock::new();

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
  serial_suffix: [char; 4],
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
    ADV_LOCAL_NAME.get_or_init(|| {
      let mut name = String::from(ADV_LOCAL_NAME_PREFIX);
      name.push(self.serial_suffix[0]);
      name.push(self.serial_suffix[1]);
      name.push(self.serial_suffix[2]);
      name.push(self.serial_suffix[3]);
      name.push(')');
      Box::leak(name.into_boxed_str())
    })
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
  pub async fn register(
    serial_suffix: [char; 4],
    adapter_dbus_path: &str,
    advertised_service_uuid: Uuid,
  ) -> Result<Self, AdvertiseError> {
    let conn = Connection::system().await?;
    let path: OwnedObjectPath = ObjectPath::try_from(ADV_OBJECT_PATH)?.into();

    conn
      .object_server()
      .at(
        &path,
        LeAdvertisementImpl {
          serial_suffix,
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

  pub async fn unregister(self) {
    let conn = self.conn.clone();
    let path = self.path.clone();
    let adapter_path = self.adapter_path.clone();
    std::mem::forget(self);
    teardown(conn, path, adapter_path).await;
  }
}

async fn teardown(conn: Connection, path: OwnedObjectPath, adapter_path: OwnedObjectPath) {
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
}

impl Drop for LeAdvertisement {
  fn drop(&mut self) {
    let conn = self.conn.clone();
    let path = self.path.clone();
    let adapter_path = self.adapter_path.clone();
    tokio::spawn(teardown(conn, path, adapter_path));
  }
}

#[derive(Debug, thiserror::Error)]
pub enum AdvertiseError {
  #[error(transparent)]
  Zbus(#[from] zbus::Error),
  #[error(transparent)]
  ZvariantPath(#[from] zbus::zvariant::Error),
}
