//! Reference companion-side gateway for bridgething. Connects to the
//! daemon's [`BRIDGETHING_NETWORK_GATEWAY_PORT`] WebSocket and speaks
//! the gateway protocol, with deliberate failure-injection knobs so
//! the daemon's bulk-transfer and OTA paths can be exercised against
//! realistic disruption without involving Bluetooth.

mod chaos;
mod conn;
mod install;
mod ota;
mod transfer;
mod webapp;

use std::path::PathBuf;

use clap::{Parser, Subcommand};
use libbridgething::OtaKind;
use uuid::Uuid;

use crate::chaos::ChaosConfig;

#[derive(Parser, Debug)]
#[command(version, about = "bridgething host gateway (dev iteration tool)", long_about = None)]
struct Cli {
  /// WebSocket URL of the daemon's network gateway.
  #[arg(long, default_value = "ws://127.0.0.1:8892/")]
  url: String,

  /// Bytes per outbound `TransferFragment`. Each fragment becomes one
  /// wire message on the Background lane. 64 KiB matches libswupdate's
  /// IPC chunk size and the daemon's ChunkedTransfer write granularity.
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

  /// If set, pace the outbound link to this many bytes per second.
  /// ~66000 emulates the phone's BT/EA link for wireless-OTA rehearsal.
  #[arg(long)]
  throttle: Option<u64>,

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
  /// Open `OtaBegin` for an image-kind update, stream the `.swu` as
  /// `TransferFragment` events on the Background lane, then watch
  /// progress until the daemon hits `Reboot` (or fails). The canonical
  /// OTA test rig.
  PushUpdate {
    /// Path to the `.swu`. Relative paths resolve against `--fixture`
    /// when set, otherwise CWD.
    swu: PathBuf,
    /// Optional update-URL base recorded in `OtaBegin.update_url_base`
    /// (server prefix the companion may refetch the .zck delta from on
    /// cache miss; daemon does not fetch).
    #[arg(long)]
    update_url_base: Option<String>,
    /// Optional rootfs .zck delta source, served for `OtaAssetRange`
    /// requests whose asset is `system.img.zck`. When unset, those range
    /// requests are rejected.
    #[arg(long)]
    zck: Option<PathBuf>,
    /// Optional boot-partition .zck delta source, served for
    /// `OtaAssetRange` requests whose asset is `boot.vfat.zck`.
    #[arg(long)]
    boot_zck: Option<PathBuf>,
  },
  /// Open `OtaBegin` for a daemon-kind update and stream a fresh aarch64 daemon binary.
  /// Daemon reaches `Writing` then `Reboot`. No range proxy traffic.
  PushDaemon {
    /// Path to the aarch64 daemon binary. Relative paths resolve
    /// against `--fixture` when set, otherwise CWD.
    binary: PathBuf,
  },
  /// Open `OtaBegin` for a builtin-webapp update and stream a hub or stock webapp bundle zip.
  /// Bundle's `manifest.json` id must be `HUB_WEBAPP_ID` or `STOCK_WEBAPP_ID`.
  PushBuiltinWebapp {
    /// Path to the webapp bundle `.zip`. Relative paths resolve
    /// against `--fixture` when set, otherwise CWD.
    bundle: PathBuf,
  },
  /// Send `WebappSwitchTo` to flip the kiosk's active webapp. The
  /// daemon rescans `/var/bridgething/webapps/` if the id is unknown,
  /// so this is also the activation half of the dev-iter loop after
  /// rsyncing a new bundle into place.
  SwitchWebapp {
    /// Webapp uuid (from the bundle's `manifest.json`).
    id: Uuid,
  },
  /// Open `OtaBegin { kind: InstalledWebapp }`, stream the `.zip` as
  /// `TransferFragment` events on the Background lane, and watch for the terminal
  /// `WebappInstalled` event (or an `OtaError`). The bundle's manifest.id
  /// determines where the daemon installs it; reserved-uuid bundles
  /// (stock/hub/launcher) are hard-rejected.
  Install {
    /// Path to the bundle `.zip`. Relative paths resolve against
    /// `--fixture` when set, otherwise CWD.
    bundle: PathBuf,
    /// Provenance to record against the install, conventionally the
    /// catalog source URL. Omitted records unknown provenance, which
    /// also clears any provenance a prior install recorded.
    #[arg(long)]
    provenance: Option<String>,
  },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
  init_logging();
  let cli = Cli::parse();

  let chaos = ChaosConfig {
    inject_loss: cli.inject_loss,
    inject_disconnect: cli.inject_disconnect.map(std::time::Duration::from_secs),
    throttle_bytes_per_sec: cli.throttle,
  };

  match cli.cmd {
    Command::Connect => conn::run_connect(&cli.url, chaos).await,
    Command::PushUpdate {
      swu,
      update_url_base,
      zck,
      boot_zck,
    } => {
      let swu_path = resolve_path(cli.fixture.as_deref(), &swu);
      let mut zcks = std::collections::HashMap::new();
      if let Some(z) = zck {
        zcks.insert("system.img.zck".to_string(), resolve_path(cli.fixture.as_deref(), &z));
      }
      if let Some(b) = boot_zck {
        zcks.insert("boot.vfat.zck".to_string(), resolve_path(cli.fixture.as_deref(), &b));
      }
      ota::run_push_update(
        &cli.url,
        chaos,
        cli.chunk_size,
        OtaKind::Image,
        swu_path,
        update_url_base,
        zcks,
      )
      .await
    }
    Command::PushDaemon { binary } => {
      let binary_path = resolve_path(cli.fixture.as_deref(), &binary);
      ota::run_push_update(
        &cli.url,
        chaos,
        cli.chunk_size,
        OtaKind::Daemon,
        binary_path,
        None,
        std::collections::HashMap::new(),
      )
      .await
    }
    Command::PushBuiltinWebapp { bundle } => {
      let bundle_path = resolve_path(cli.fixture.as_deref(), &bundle);
      ota::run_push_update(
        &cli.url,
        chaos,
        cli.chunk_size,
        OtaKind::BuiltinWebapp,
        bundle_path,
        None,
        std::collections::HashMap::new(),
      )
      .await
    }
    Command::SwitchWebapp { id } => webapp::run_switch(&cli.url, chaos, id).await,
    Command::Install { bundle, provenance } => {
      let bundle_path = resolve_path(cli.fixture.as_deref(), &bundle);
      install::run_install(&cli.url, chaos, cli.chunk_size, bundle_path, provenance).await
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
