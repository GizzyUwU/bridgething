use std::collections::HashMap;

use libbridgething::Device;
use sea_orm::{DatabaseConnection, EntityTrait, IntoActiveModel, TransactionTrait};

use super::{
  super::{StateError, StateResult},
  device::{Column as DeviceColumn, Entity as DeviceEntity, Model as DeviceModel},
  meta::{Column as MetaColumn, Entity as MetaEntity, KEY_LAST_DEVICE},
};

#[derive(Debug, Clone)]
pub struct DeviceStore {
  db: DatabaseConnection,
}

impl DeviceStore {
  pub fn new(db: DatabaseConnection) -> Self {
    Self { db }
  }

  pub async fn list(&self) -> StateResult<HashMap<String, Device>> {
    let rows = DeviceEntity::find().all(&self.db).await.map_err(StateError::from)?;
    Ok(rows.iter().map(|m| (m.mac.clone(), Device::from(m))).collect())
  }

  pub async fn get(&self, mac: &str) -> StateResult<Option<Device>> {
    Ok(
      DeviceEntity::find_by_id(mac.to_string())
        .one(&self.db)
        .await?
        .as_ref()
        .map(Device::from),
    )
  }

  pub async fn upsert(&self, device: Device) -> StateResult<()> {
    let model = DeviceModel::from_wire(&device).into_active_model();
    DeviceEntity::insert(model)
      .on_conflict(
        sea_orm::sea_query::OnConflict::column(DeviceColumn::Mac)
          .update_columns([DeviceColumn::Name, DeviceColumn::DeviceType, DeviceColumn::IsDefault])
          .to_owned(),
      )
      .exec(&self.db)
      .await?;
    Ok(())
  }

  pub async fn remove(&self, mac: String) -> StateResult<()> {
    let tx = self.db.begin().await?;
    DeviceEntity::delete_by_id(mac.clone()).exec(&tx).await?;
    let last = MetaEntity::find_by_id(KEY_LAST_DEVICE.to_string()).one(&tx).await?;
    if last.map(|m| m.value) == Some(mac) {
      MetaEntity::delete_by_id(KEY_LAST_DEVICE.to_string()).exec(&tx).await?;
    }
    tx.commit().await?;
    Ok(())
  }

  pub async fn last(&self) -> StateResult<Option<String>> {
    Ok(
      MetaEntity::find_by_id(KEY_LAST_DEVICE.to_string())
        .one(&self.db)
        .await?
        .map(|m| m.value),
    )
  }

  pub async fn set_last(&self, mac: String) -> StateResult<()> {
    let model = super::meta::ActiveModel {
      key: sea_orm::Set(KEY_LAST_DEVICE.to_string()),
      value: sea_orm::Set(mac),
    };
    MetaEntity::insert(model)
      .on_conflict(
        sea_orm::sea_query::OnConflict::column(MetaColumn::Key)
          .update_column(MetaColumn::Value)
          .to_owned(),
      )
      .exec(&self.db)
      .await?;
    Ok(())
  }
}
