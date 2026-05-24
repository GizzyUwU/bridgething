//! Tiny LE GATT server hosting one read-only characteristic flagged
//! `encrypt-read`. The service UUID is placed in the LE advertisement's
//! `ServiceUUIDs` list (`advertise.rs`) so the iOS companion app's
//! `AccessorySetupKit` discovery descriptor and CoreBluetooth's
//! `scanForPeripherals(withServices:)` filter can find us — both match
//! advertised service UUIDs, not the LE Service-Solicitation AD type.
//!
//! `AccessorySetupKit` pairs LE inside the picker process itself, so
//! the encrypt-read trick is not the SMP trigger on iOS 18+. The
//! characteristic survives as a defensive fallback: any unbonded peer
//! that reads it bounces with InsufficientAuthentication, forcing iOS
//! to drive SMP. Empty value by design — the side effect of being
//! unreadable until the link is encrypted is the only thing we want.
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
      "LE pair-trigger GATT service registered"
    );
    Ok(Self { _handle: handle })
  }
}
