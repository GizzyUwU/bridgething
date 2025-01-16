use bridgething_adapter::scan_and_connect;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
  scan_and_connect().await?;

  Ok(())
}
