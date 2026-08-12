#![cfg(feature = "emulator")]

use std::time::Duration;

use bridgething_iap2::{
  SessionEvent,
  emulator::harness::{self as emu, recv_with_timeout},
};

#[tokio::test]
async fn emulator_drives_accessory_to_identified() {
  let _ = tracing_subscriber::fmt()
    .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
    .with_test_writer()
    .try_init();

  let (mut harness, _emu_events, _emu_handle) = emu::spawn(emu::identification_config(), None, |e| e);

  let mut authenticated = false;
  loop {
    let evt = recv_with_timeout(&mut harness.acc_events, Duration::from_secs(10))
      .await
      .expect("accessory event timed out or closed before Identified");
    match evt {
      SessionEvent::LinkEstablished(_) => continue,
      SessionEvent::Authenticated => authenticated = true,
      SessionEvent::Identified => {
        assert!(authenticated, "accessory must authenticate before identifying");
        return;
      }
      other => panic!("expected the auth->ident chain, got {other:?}"),
    }
  }
}
