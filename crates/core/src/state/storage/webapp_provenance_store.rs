use std::collections::HashMap;

use sea_orm::{DatabaseConnection, EntityTrait, Set};
use uuid::Uuid;

use super::{
  super::StateResult,
  webapp_provenance::{Column as ProvenanceColumn, Entity as ProvenanceEntity},
};

#[derive(Debug, Clone)]
pub struct WebappProvenanceStore {
  db: DatabaseConnection,
}

impl WebappProvenanceStore {
  pub fn new(db: DatabaseConnection) -> Self {
    Self { db }
  }

  pub async fn all(&self) -> StateResult<HashMap<Uuid, String>> {
    let rows = ProvenanceEntity::find().all(&self.db).await?;
    Ok(
      rows
        .into_iter()
        .filter_map(|row| Uuid::parse_str(&row.webapp_id).ok().map(|id| (id, row.provenance)))
        .collect(),
    )
  }

  pub async fn set(&self, id: Uuid, provenance: Option<&str>) -> StateResult<()> {
    match provenance {
      Some(value) => {
        let model = super::webapp_provenance::ActiveModel {
          webapp_id: Set(id.to_string()),
          provenance: Set(value.to_string()),
        };
        ProvenanceEntity::insert(model)
          .on_conflict(
            sea_orm::sea_query::OnConflict::column(ProvenanceColumn::WebappId)
              .update_column(ProvenanceColumn::Provenance)
              .to_owned(),
          )
          .exec(&self.db)
          .await?;
      }
      None => self.clear(id).await?,
    }
    Ok(())
  }

  pub async fn clear(&self, id: Uuid) -> StateResult<()> {
    ProvenanceEntity::delete_by_id(id.to_string()).exec(&self.db).await?;
    Ok(())
  }
}
