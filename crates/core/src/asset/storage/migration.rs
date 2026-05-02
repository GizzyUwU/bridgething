use sea_orm_migration::prelude::*;

pub fn migrations() -> Vec<Box<dyn MigrationTrait>> {
  vec![Box::new(M0001CreateAssets)]
}

struct M0001CreateAssets;

impl MigrationName for M0001CreateAssets {
  fn name(&self) -> &str {
    "m0001_create_assets"
  }
}

#[async_trait::async_trait]
impl MigrationTrait for M0001CreateAssets {
  async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
    manager
      .create_table(
        Table::create()
          .table(Assets::Table)
          .if_not_exists()
          .col(ColumnDef::new(Assets::Id).text().not_null().primary_key())
          .col(ColumnDef::new(Assets::Bytes).blob().not_null())
          .col(ColumnDef::new(Assets::Mime).text())
          .col(ColumnDef::new(Assets::ByteLen).big_integer().not_null())
          .col(ColumnDef::new(Assets::InsertedAt).big_integer().not_null())
          .col(ColumnDef::new(Assets::AccessedAt).big_integer().not_null())
          .to_owned(),
      )
      .await?;

    manager
      .create_index(
        Index::create()
          .if_not_exists()
          .name("idx_assets_accessed_at")
          .table(Assets::Table)
          .col(Assets::AccessedAt)
          .to_owned(),
      )
      .await
  }

  async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
    manager
      .drop_index(Index::drop().name("idx_assets_accessed_at").to_owned())
      .await?;
    manager.drop_table(Table::drop().table(Assets::Table).to_owned()).await
  }
}

#[derive(DeriveIden)]
enum Assets {
  Table,
  Id,
  Bytes,
  Mime,
  ByteLen,
  InsertedAt,
  AccessedAt,
}
