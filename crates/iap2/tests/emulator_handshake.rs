#![cfg(feature = "emulator")]

use std::time::Duration;

use bridgething_iap2::{Iap2Command, Iap2Event, Link, LinkConfig, Lsp, SessionTriple};
use tokio::sync::mpsc;

fn fast_config(initial_psn: u8, lsp: Lsp) -> LinkConfig {
  let mut config = LinkConfig::new(lsp);
  config.initial_psn = initial_psn;
  config.detect_interval = Duration::from_millis(20);
  config.handshake_timeout = Duration::from_secs(5);
  config
}

fn device_lsp() -> Lsp {
  Lsp {
    version: 1,
    max_outgoing: 127,
    max_len: 65535,
    retransmission_timeout_ms: 6000,
    ack_timeout_ms: 3000,
    max_retransmissions: 30,
    max_ack: 3,
    sessions: vec![
      SessionTriple {
        id: 1,
        session_type: 0,
        version: 1,
      },
      SessionTriple {
        id: 2,
        session_type: 1,
        version: 2,
      },
      SessionTriple {
        id: 3,
        session_type: 2,
        version: 1,
      },
    ],
  }
}

#[tokio::test]
async fn accessory_and_device_complete_handshake() {
  let (acc_io, dev_io) = tokio::io::duplex(8192);

  let acc_lsp = Lsp::accessory_default();
  let dev_lsp = device_lsp();

  let (acc_ev_tx, mut acc_ev_rx) = mpsc::channel(16);
  let (acc_cmd_tx, acc_cmd_rx) = mpsc::channel::<Iap2Command>(16);
  let (dev_ev_tx, mut dev_ev_rx) = mpsc::channel(16);
  let (dev_cmd_tx, dev_cmd_rx) = mpsc::channel::<Iap2Command>(16);

  tokio::spawn(Link::run(
    acc_io,
    fast_config(99, acc_lsp.clone()),
    acc_ev_tx,
    acc_cmd_rx,
  ));
  tokio::spawn(Link::run_device(
    dev_io,
    fast_config(215, dev_lsp.clone()),
    dev_ev_tx,
    dev_cmd_rx,
  ));

  let acc_first = tokio::time::timeout(Duration::from_secs(5), acc_ev_rx.recv())
    .await
    .expect("accessory handshake timed out")
    .expect("accessory link closed before Established");
  match acc_first {
    Iap2Event::Established(lsp) => assert_eq!(lsp, dev_lsp, "accessory must learn the device's LSP"),
    other => panic!("accessory expected Established, got {other:?}"),
  }

  let dev_first = tokio::time::timeout(Duration::from_secs(5), dev_ev_rx.recv())
    .await
    .expect("device handshake timed out")
    .expect("device link closed before Established");
  match dev_first {
    Iap2Event::Established(lsp) => assert_eq!(lsp, acc_lsp, "device must learn the accessory's LSP"),
    other => panic!("device expected Established, got {other:?}"),
  }

  drop(acc_cmd_tx);
  drop(dev_cmd_tx);
}
