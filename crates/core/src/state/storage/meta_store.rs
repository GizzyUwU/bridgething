use sea_orm::{DatabaseConnection, EntityTrait, Set};
use uuid::Uuid;

use super::{
  super::{StateResult, webapps::WebappRegistry},
  meta::{Column as MetaColumn, Entity as MetaEntity, KEY_ACTIVE_WEBAPP},
};

#[derive(Debug, Clone)]
pub struct MetaStore {
  db: DatabaseConnection,
}

impl MetaStore {
  pub fn new(db: DatabaseConnection) -> Self {
    Self { db }
  }

  pub async fn active_webapp(&self, webapps: &WebappRegistry) -> StateResult<Option<Uuid>> {
    let stored = self.read_meta(KEY_ACTIVE_WEBAPP).await?;
    let parsed = stored.as_deref().and_then(|s| Uuid::parse_str(s).ok());
    if let Some(id) = parsed
      && webapps.resolve(id).await.is_some()
    {
      return Ok(Some(id));
    }
    Ok(webapps.default_id().await)
  }

  pub async fn set_active_webapp(&self, id: Uuid) -> StateResult<()> {
    self.write_meta(KEY_ACTIVE_WEBAPP, &id.simple().to_string()).await
  }

  pub async fn enforce_active_webapp_exists(&self, webapps: &WebappRegistry) -> StateResult<()> {
    let stored = self.read_meta(KEY_ACTIVE_WEBAPP).await?;
    let parsed = stored.as_deref().and_then(|s| Uuid::parse_str(s).ok());
    if let Some(id) = parsed
      && webapps.resolve(id).await.is_some()
    {
      return Ok(());
    }
    match webapps.default_id().await {
      Some(id) => {
        tracing::warn!(
          "persisted active webapp ({:?}) does not resolve; falling back to {}",
          stored,
          id
        );
        self.write_meta(KEY_ACTIVE_WEBAPP, &id.simple().to_string()).await?;
      }
      None => {
        tracing::warn!("no webapps installed and no builtin available; clearing active webapp meta");
        MetaEntity::delete_by_id(KEY_ACTIVE_WEBAPP.to_string())
          .exec(&self.db)
          .await?;
      }
    }
    Ok(())
  }

  async fn read_meta(&self, key: &str) -> StateResult<Option<String>> {
    Ok(
      MetaEntity::find_by_id(key.to_string())
        .one(&self.db)
        .await?
        .map(|m| m.value),
    )
  }

  async fn write_meta(&self, key: &str, value: &str) -> StateResult<()> {
    let model = super::meta::ActiveModel {
      key: Set(key.to_string()),
      value: Set(value.to_string()),
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
