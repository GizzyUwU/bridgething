use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "webapp_provenance")]
pub struct Model {
  #[sea_orm(primary_key, auto_increment = false)]
  pub webapp_id: String,
  pub provenance: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
