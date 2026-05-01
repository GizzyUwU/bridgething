use std::path::Path;

use sea_orm::{ConnectOptions, ConnectionTrait, Database, DatabaseConnection, DbErr, Statement};
use sea_orm_migration::MigratorTrait;

pub mod entity;
mod migration;

pub use entity::{ActiveModel as AssetActiveModel, Column as AssetColumn, Entity as AssetEntity, Model as AssetModel};

/// Open the daemon's persistent sqlite database, run migrations, and
/// return a connection handle. The database file is created if missing
/// and any pending migrations are applied. WAL is enabled so concurrent
/// readers don't block the writer (the cache actor is the sole writer
/// today; future state migrations will introduce more readers).
pub async fn open_db(path: &Path) -> Result<DatabaseConnection, DbErr> {
  if let Some(parent) = path.parent()
    && !parent.exists()
  {
    std::fs::create_dir_all(parent).map_err(|e| DbErr::Custom(format!("create db parent dir: {e}")))?;
  }

  let url = format!("sqlite://{}?mode=rwc", path.display());
  let mut opts = ConnectOptions::new(url);
  opts.sqlx_logging(false);

  let db = Database::connect(opts).await?;

  db.execute(Statement::from_string(
    db.get_database_backend(),
    "PRAGMA journal_mode=WAL;",
  ))
  .await?;
  db.execute(Statement::from_string(
    db.get_database_backend(),
    "PRAGMA synchronous=NORMAL;",
  ))
  .await?;

  migration::Migrator::up(&db, None).await?;
  Ok(db)
}

#[cfg(test)]
pub async fn open_db_for_tests(db: &DatabaseConnection) -> Result<(), DbErr> {
  migration::Migrator::up(db, None).await
}
