use bluer::agent::{
  Agent, AgentHandle, AuthorizeService, DisplayPasskey, DisplayPinCode, ReqResult, RequestAuthorization,
  RequestConfirmation, RequestPasskey, RequestPinCode,
};

use super::profiles::{BluetoothConnectionEvent, ProfileMan};

pub async fn request_authorization(profile: ProfileMan, req: RequestAuthorization) -> ReqResult<()> {
  tracing::info!(
    "pairing authorization requested from device {} on adapter {}",
    &req.device,
    &req.adapter
  );

  if let Err(err) = profile
    .handle_event(BluetoothConnectionEvent::AuthRequest { mac: req.device })
    .await
  {
    tracing::error!("failed to send bluetooth msg: {:?}", err);
  }

  Ok(())
}

pub async fn request_confirmation(profile: ProfileMan, req: RequestConfirmation) -> ReqResult<()> {
  tracing::info!(
    "pairing confirmation requested from device {} on adapter {} with passkey {}",
    &req.device,
    &req.adapter,
    &req.passkey,
  );

  if let Err(err) = profile
    .handle_event(BluetoothConnectionEvent::PinCode {
      mac: req.device,
      pin: format!("\"{:06}\"", req.passkey),
    })
    .await
  {
    tracing::error!("failed to send bluetooth msg: {:?}", err);
  }

  Ok(())
}

pub async fn authorize_service(profile: ProfileMan, req: AuthorizeService) -> ReqResult<()> {
  tracing::debug!(
    "service authorization requested from {} on adapter {} for service {}",
    &req.device,
    &req.adapter,
    &req.service
  );

  if let Err(err) = profile
    .handle_event(BluetoothConnectionEvent::ServiceAuthRequest {
      mac: req.device,
      service: req.service,
    })
    .await
  {
    tracing::error!("failed to send bluetooth msg: {:?}", err);
  }

  Ok(())
}

pub async fn display_pin_code(profile: ProfileMan, req: DisplayPinCode) -> ReqResult<()> {
  tracing::info!(
    "pairing pin code for device {} on {} is \"{}\"",
    &req.device,
    &req.adapter,
    req.pincode
  );

  if let Err(err) = profile
    .handle_event(BluetoothConnectionEvent::PinCode {
      mac: req.device,
      pin: req.pincode,
    })
    .await
  {
    tracing::error!("failed to send bluetooth msg: {:?}", err);
  }

  Ok(())
}

pub async fn display_passkey(profile: ProfileMan, req: DisplayPasskey) -> ReqResult<()> {
  tracing::info!(
    "pairing passkey for device {} on {} is \"{:06}\"",
    &req.device,
    &req.adapter,
    req.passkey
  );

  // yes i know passkey and pin are different no i don't care
  if let Err(err) = profile
    .handle_event(BluetoothConnectionEvent::PinCode {
      mac: req.device,
      pin: format!("\"{:06}\"", req.passkey),
    })
    .await
  {
    tracing::error!("failed to send bluetooth msg: {:?}", err);
  }

  Ok(())
}

pub async fn handle_request_passkey(_: ProfileMan, req: RequestPasskey) -> ReqResult<u32> {
  tracing::info!(
    "pairing passkey requested for device {} on {}",
    &req.device,
    &req.adapter
  );

  Ok(696969)
}

pub async fn handle_request_pincode(_: ProfileMan, req: RequestPinCode) -> ReqResult<String> {
  tracing::info!(
    "pairing pincode requested for device {} on {}",
    &req.device,
    &req.adapter
  );

  Ok("696969".to_string())
}

pub async fn build_agent(session: &bluer::Session, profile: ProfileMan) -> bluer::Result<AgentHandle> {
  let agent = Agent {
    request_default: true,
    display_pin_code: Some(Box::new({
      let profile = profile.clone();
      move |req| Box::pin(display_pin_code(profile.clone(), req))
    })),
    display_passkey: Some(Box::new({
      let profile = profile.clone();
      move |req| Box::pin(display_passkey(profile.clone(), req))
    })),
    request_confirmation: Some(Box::new({
      let profile = profile.clone();
      move |req| Box::pin(request_confirmation(profile.clone(), req))
    })),
    request_authorization: Some(Box::new({
      let profile = profile.clone();
      move |req| Box::pin(request_authorization(profile.clone(), req))
    })),
    authorize_service: Some(Box::new({
      let profile = profile.clone();
      move |req| Box::pin(authorize_service(profile.clone(), req))
    })),
    request_passkey: Some(Box::new({
      let profile = profile.clone();
      move |req| Box::pin(handle_request_passkey(profile.clone(), req))
    })),
    request_pin_code: Some(Box::new({
      let profile = profile.clone();
      move |req| Box::pin(handle_request_pincode(profile.clone(), req))
    })),
    ..Default::default()
  };

  session.register_agent(agent).await
}
