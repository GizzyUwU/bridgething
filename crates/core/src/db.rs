use std::path::Path;

use sea_orm::{ConnectOptions, ConnectionTrait, Database, DatabaseConnection, DbErr, Statement};
use sea_orm_migration::{MigrationTrait, MigratorTrait, async_trait};

use crate::{asset, state};

pub async fn open(path: Option<&Path>) -> Result<DatabaseConnection, DbErr> {
  let url = match path {
    Some(p) => {
      if let Some(parent) = p.parent()
        && !parent.exists()
      {
        std::fs::create_dir_all(parent).map_err(|e| DbErr::Custom(format!("create db parent dir: {e}")))?;
      }
      format!("sqlite://{}?mode=rwc", p.display())
    }
    None => "sqlite::memory:".to_string(),
  };

  let mut opts = ConnectOptions::new(url);
  opts.sqlx_logging(false);
  let db = Database::connect(opts).await?;

  if path.is_some() {
    db.execute(Statement::from_string(
      db.get_database_backend(),
      "PRAGMA journal_mode=WAL;",
    ))
    .await?;
    db.execute(Statement::from_string(
      db.get_database_backend(),
      "PRAGMA synchronous=FULL;",
    ))
    .await?;
  }

  Migrator::up(&db, None).await?;
  Ok(db)
}

struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
  fn migrations() -> Vec<Box<dyn MigrationTrait>> {
    let mut all = Vec::new();
    all.extend(asset::storage::migration::migrations());
    all.extend(state::storage::migration::migrations());
    all
  }
}
