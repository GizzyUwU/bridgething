//! Tiny LE GATT server hosting one read-only characteristic flagged
//! `encrypt-read`. The companion iOS app's CoreBluetooth
//! flow uses this as the trigger for SMP: after LE-connecting to the
//! daemon, the app reads this characteristic, BlueZ rejects the read
//! with InsufficientAuthentication, iOS drives the LE pair prompt,
//! the bond completes, and the daemon's GATT-client side picks up
//! ANCS over the new LE-bonded link.
//!
//! The characteristic value is empty by design — its only job is to
//! be readable on an encrypted link, which forces SMP on first
//! attempt from an unbonded peer.
//!
//! The service UUID is also placed in the LE advertisement's
//! ServiceUUIDs list so the companion app can filter discovery on
//! it (CoreBluetooth's `scanForPeripherals(withServices:)` matches
//! against advertised service UUIDs, not solicited ones).
//!
//! Public so the companion-app SDKs can import the same constant.
//! Once a stable wire-surface needs it, the constant moves to
//! `libbridgething`.

use bluer::{
  Adapter, Uuid,
  gatt::local::{Application, ApplicationHandle, Characteristic, CharacteristicRead, Service},
};
use futures::FutureExt;

pub const PAIR_TRIGGER_SERVICE: Uuid = Uuid::from_u128(0xb12be732_c1d0_4001_8001_bb1d6e7a1c01);
pub const PAIR_TRIGGER_CHAR: Uuid = Uuid::from_u128(0xb12be732_c1d0_4001_8001_bb1d6e7a1c02);

pub struct PairTrigger {
  _handle: ApplicationHandle,
}

impl PairTrigger {
  pub async fn register(adapter: &Adapter) -> bluer::Result<Self> {
    let app = Application {
      services: vec![Service {
        uuid: PAIR_TRIGGER_SERVICE,
        primary: true,
        characteristics: vec![Characteristic {
          uuid: PAIR_TRIGGER_CHAR,
          read: Some(CharacteristicRead {
            read: true,
            encrypt_read: true,
            fun: Box::new(|_| async move { Ok(Vec::new()) }.boxed()),
            ..Default::default()
          }),
          ..Default::default()
        }],
        ..Default::default()
      }],
      ..Default::default()
    };
    let handle = adapter.serve_gatt_application(app).await?;
    tracing::info!(
      service = %PAIR_TRIGGER_SERVICE,
      "ANCS pair-trigger GATT service registered"
    );
    Ok(Self { _handle: handle })
  }
}
