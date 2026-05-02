//! Reference companion-side gateway for bridgething. Connects to the
//! daemon's [`BRIDGETHING_NETWORK_GATEWAY_PORT`] WebSocket and speaks
//! the gateway protocol, with deliberate failure-injection knobs so
//! the daemon's bulk-transfer and OTA paths can be exercised against
//! realistic disruption without involving Bluetooth.

mod chaos;
mod conn;
mod ota;

use std::path::PathBuf;

use clap::{Parser, Subcommand};

use crate::chaos::ChaosConfig;

#[derive(Parser, Debug)]
#[command(version, about = "bridgething host gateway (dev iteration tool)", long_about = None)]
struct Cli {
  /// WebSocket URL of the daemon's network gateway.
  #[arg(long, default_value = "ws://127.0.0.1:8892/")]
  url: String,

  /// Soft cap on the bytes pushed per asset chunk. Currently informational
  /// (the daemon-side path uses a single `AssetPush` per blob); kept here
  /// so future chunked-push paths share a knob.
  #[arg(long, default_value_t = 64 * 1024)]
  chunk_size: usize,

  /// Probability (0.0-1.0) of dropping an outbound frame at the codec
  /// edge. Useful for proving that protocol-level retries / handler
  /// timeouts behave under unreliable transport.
  #[arg(long, default_value_t = 0.0)]
  inject_loss: f32,

  /// If set, close the connection after this many seconds. Useful for
  /// exercising mid-write disconnect handling on the daemon side.
  #[arg(long)]
  inject_disconnect: Option<u64>,

  /// Optional fixtures directory. Subcommands resolve relative paths
  /// against this when set.
  #[arg(long)]
  fixture: Option<PathBuf>,

  #[command(subcommand)]
  cmd: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
  /// Connect, exchange Version, then print every inbound frame until
  /// killed. Outbound traffic is just the initial Version reply.
  Connect,
  /// Push a `.swu` artifact via AssetCache, send `ApplyUpdate`, then
  /// stream OTA progress events to stdout. Intended as the canonical
  /// OTA test rig.
  PushUpdate {
    /// Path to the `.swu`. Relative paths resolve against `--fixture`
    /// when set, otherwise CWD.
    swu: PathBuf,
    /// Optional manifest URL to record in `ApplyUpdate.manifest_url`
    /// (telemetry only - the daemon does not fetch).
    #[arg(long)]
    manifest_url: Option<String>,
    /// AssetCache id to push under. Defaults to a deterministic hash
    /// of the file path.
    #[arg(long)]
    asset_id: Option<String>,
  },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
  init_logging();
  let cli = Cli::parse();

  let chaos = ChaosConfig {
    inject_loss: cli.inject_loss,
    inject_disconnect: cli.inject_disconnect.map(std::time::Duration::from_secs),
  };

  match cli.cmd {
    Command::Connect => conn::run_connect(&cli.url, chaos).await,
    Command::PushUpdate {
      swu,
      manifest_url,
      asset_id,
    } => {
      let path = resolve_path(cli.fixture.as_deref(), &swu);
      ota::run_push_update(&cli.url, chaos, cli.chunk_size, path, manifest_url, asset_id).await
    }
  }
}

fn resolve_path(fixture: Option<&std::path::Path>, p: &std::path::Path) -> PathBuf {
  if p.is_absolute() {
    return p.to_path_buf();
  }
  match fixture {
    Some(dir) => dir.join(p),
    None => p.to_path_buf(),
  }
}

fn init_logging() {
  let filter = tracing_subscriber::EnvFilter::try_from_default_env()
    .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("host_gateway=info,bridgething_host_gateway=info,info"));
  tracing_subscriber::fmt().with_env_filter(filter).init();
}
