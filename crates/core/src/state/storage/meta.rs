use sea_orm::entity::prelude::*;

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
pub const KEY_LAUNCHER_WEBAPP: &str = "launcher_webapp";
pub const KEY_OVERLAY_WEBAPP: &str = "overlay_webapp";
pub const KEY_LAST_DEVICE: &str = "last_device";
