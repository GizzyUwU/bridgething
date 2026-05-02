use sea_orm::entity::prelude::*;

/// Single-row scalar values that don't deserve their own table:
/// `active_webapp` and `last_device`. Schemaless on the value side -
/// callers serialize/deserialize the string they expect.
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "meta")]
pub struct Model {
  #[sea_orm(primary_key, auto_increment = false)]
  pub key: String,
  pub value: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

pub const KEY_ACTIVE_WEBAPP: &str = "active_webapp";
pub const KEY_LAST_DEVICE: &str = "last_device";
