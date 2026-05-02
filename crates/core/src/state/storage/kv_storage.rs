use sea_orm::entity::prelude::*;

/// The webapp-facing KV store. Stock-app calls land here via the
/// storage handler; modern webapps reach the same store through the
/// store surface. Values are opaque strings.
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "kv_storage")]
pub struct Model {
  #[sea_orm(primary_key, auto_increment = false)]
  pub key: String,
  pub value: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
