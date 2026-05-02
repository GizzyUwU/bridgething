//! Daemon-wide sqlite connection and unified migrator.
//!
//! One database file (`bridgething.db` under `state_dir`) backs both
//! the asset cache and the persistent app state. WAL is on so the
//! actor-owned asset writer doesn't block AppState's CRUD reads. The
//! `synchronous=NORMAL` pragma trades the rare-edge "lose the last
//! committed transaction on power-loss" guarantee for a large fsync
//! reduction on every commit; whole-database corruption stays
//! impossible the way fsync-on-every-write file-blob persistence does
//! not.

use std::path::Path;

use sea_orm::{ConnectOptions, ConnectionTrait, Database, DatabaseConnection, DbErr, Statement};
use sea_orm_migration::{MigrationTrait, MigratorTrait, async_trait};

use crate::{asset, state};

/// Open the daemon's persistent sqlite database, run all pending
/// migrations from every domain (asset cache, app state), and return
/// a connection handle that callers can clone freely.
///
/// Pass `None` to open an in-memory database; this is the no-persist
/// path and is identical at the SQL layer to the on-disk version.
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
      "PRAGMA synchronous=NORMAL;",
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
