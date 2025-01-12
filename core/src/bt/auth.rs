use bluer::agent::{
  Agent, AgentHandle, AuthorizeService, DisplayPasskey, DisplayPinCode, ReqResult, RequestAuthorization,
  RequestConfirmation,
};

use crate::bt::BluetoothEvent;

use super::BluetoothTx;

pub async fn request_authorization(tx: BluetoothTx, req: RequestAuthorization) -> ReqResult<()> {
  tracing::info!(
    "pairing authorization requested from device {} on adapter {}",
    &req.device,
    &req.adapter
  );

  if let Err(err) = tx.send(BluetoothEvent::AuthRequest { mac: req.device }).await {
    tracing::error!("failed to send bluetooth msg: {:?}", err);
  }

  Ok(())
}

pub async fn request_confirmation(tx: BluetoothTx, req: RequestConfirmation) -> ReqResult<()> {
  tracing::info!(
    "pairing confirmation requested from device {} on adapter {} with passkey {}",
    &req.device,
    &req.adapter,
    &req.passkey,
  );

  if let Err(err) = tx
    .send(BluetoothEvent::PinCode {
      mac: req.device,
      pin: format!("\"{:06}\"", req.passkey),
    })
    .await
  {
    tracing::error!("failed to send bluetooth msg: {:?}", err);
  }

  Ok(())
}

pub async fn authorize_service(tx: BluetoothTx, req: AuthorizeService) -> ReqResult<()> {
  tracing::debug!(
    "service authorization requested from {} on adapter {} for service {}",
    &req.device,
    &req.adapter,
    &req.service
  );

  if let Err(err) = tx
    .send(BluetoothEvent::ServiceAuthRequest {
      mac: req.device,
      service: req.service,
    })
    .await
  {
    tracing::error!("failed to send bluetooth msg: {:?}", err);
  }

  Ok(())
}

pub async fn display_pin_code(tx: BluetoothTx, req: DisplayPinCode) -> ReqResult<()> {
  tracing::info!(
    "pairing pin code for device {} on {} is \"{}\"",
    &req.device,
    &req.adapter,
    req.pincode
  );

  if let Err(err) = tx
    .send(BluetoothEvent::PinCode {
      mac: req.device,
      pin: req.pincode,
    })
    .await
  {
    tracing::error!("failed to send bluetooth msg: {:?}", err);
  }

  Ok(())
}

pub async fn display_passkey(tx: BluetoothTx, req: DisplayPasskey) -> ReqResult<()> {
  tracing::info!(
    "pairing passkey for device {} on {} is \"{:06}\"",
    &req.device,
    &req.adapter,
    req.passkey
  );

  // yes i know passkey and pin are different but i do not care
  if let Err(err) = tx
    .send(BluetoothEvent::PinCode {
      mac: req.device,
      pin: format!("\"{:06}\"", req.passkey),
    })
    .await
  {
    tracing::error!("failed to send bluetooth msg: {:?}", err);
  }

  Ok(())
}

pub async fn build_agent(session: &bluer::Session, tx: BluetoothTx) -> bluer::Result<AgentHandle> {
  // i hate that this requires two clones - thank you borrow checker
  let agent = Agent {
    request_default: true,
    display_pin_code: Some(Box::new({
      let tx = tx.clone();
      move |req| Box::pin(display_pin_code(tx.clone(), req))
    })),
    display_passkey: Some(Box::new({
      let tx = tx.clone();
      move |req| Box::pin(display_passkey(tx.clone(), req))
    })),
    request_confirmation: Some(Box::new({
      let tx = tx.clone();
      move |req| Box::pin(request_confirmation(tx.clone(), req))
    })),
    request_authorization: Some(Box::new({
      let tx = tx.clone();
      move |req| Box::pin(request_authorization(tx.clone(), req))
    })),
    authorize_service: Some(Box::new({
      let tx = tx.clone();
      move |req| Box::pin(authorize_service(tx.clone(), req))
    })),
    ..Default::default()
  };

  session.register_agent(agent).await
}
