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
