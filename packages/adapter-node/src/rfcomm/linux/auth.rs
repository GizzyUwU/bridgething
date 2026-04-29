use bluer::agent::{
  Agent, AgentHandle, AuthorizeService, DisplayPasskey, DisplayPinCode, ReqResult, RequestAuthorization,
  RequestConfirmation, RequestPasskey, RequestPinCode,
};

pub async fn request_authorization(req: RequestAuthorization) -> ReqResult<()> {
  tracing::info!(
    "pairing authorization requested from device {} on adapter {}",
    &req.device,
    &req.adapter
  );

  Ok(())
}

pub async fn request_confirmation(req: RequestConfirmation) -> ReqResult<()> {
  tracing::info!(
    "pairing confirmation requested from device {} on adapter {} with passkey {}",
    &req.device,
    &req.adapter,
    &req.passkey,
  );

  Ok(())
}

pub async fn authorize_service(req: AuthorizeService) -> ReqResult<()> {
  tracing::debug!(
    "service authorization requested from {} on adapter {} for service {}",
    &req.device,
    &req.adapter,
    &req.service
  );

  Ok(())
}

pub async fn display_pin_code(req: DisplayPinCode) -> ReqResult<()> {
  tracing::info!(
    "pairing pin code for device {} on {} is \"{}\"",
    &req.device,
    &req.adapter,
    req.pincode
  );

  Ok(())
}

pub async fn display_passkey(req: DisplayPasskey) -> ReqResult<()> {
  tracing::info!(
    "pairing passkey for device {} on {} is \"{:06}\"",
    &req.device,
    &req.adapter,
    req.passkey
  );

  Ok(())
}

pub async fn handle_request_passkey(req: RequestPasskey) -> ReqResult<u32> {
  tracing::info!(
    "pairing passkey requested for device {} on {}",
    &req.device,
    &req.adapter
  );

  Ok(696969)
}

pub async fn handle_request_pincode(req: RequestPinCode) -> ReqResult<String> {
  tracing::info!(
    "pairing pincode requested for device {} on {}",
    &req.device,
    &req.adapter
  );

  Ok("696969".to_string())
}

pub async fn build_agent(session: &bluer::Session) -> bluer::Result<AgentHandle> {
  let agent = Agent {
    request_default: true,
    display_pin_code: Some(Box::new(move |req| Box::pin(display_pin_code(req)))),
    display_passkey: Some(Box::new(move |req| Box::pin(display_passkey(req)))),
    request_confirmation: Some(Box::new(move |req| Box::pin(request_confirmation(req)))),
    request_authorization: Some(Box::new(move |req| Box::pin(request_authorization(req)))),
    authorize_service: Some(Box::new(move |req| Box::pin(authorize_service(req)))),
    request_passkey: Some(Box::new(move |req| Box::pin(handle_request_passkey(req)))),
    request_pin_code: Some(Box::new(move |req| Box::pin(handle_request_pincode(req)))),
    ..Default::default()
  };

  session.register_agent(agent).await
}
