use anyhow::{Result, anyhow};
use bridgething_delivery::session::DeliverySession;
use bridgething_gateway::RequestFailure;
use libbridgething::gateway::WebappSwitchTo;
use uuid::Uuid;

use crate::{chaos::ChaosConfig, session};

pub async fn run_switch(url: &str, chaos: ChaosConfig, id: Uuid) -> Result<()> {
  let session = session::connect(url, chaos).await?;
  switch(&session, id).await
}

pub async fn switch(session: &DeliverySession, id: Uuid) -> Result<()> {
  match session.gateway.webapp().switch_to(WebappSwitchTo { id }).await {
    Ok(active) => {
      tracing::info!(
        id = %active.id.map(|id| id.to_string()).unwrap_or_default(),
        name = active.name.as_deref().unwrap_or("(none)"),
        "switched active webapp"
      );
      Ok(())
    }
    Err(RequestFailure::Domain(err)) => Err(anyhow!("daemon rejected switch: {err:?}")),
    Err(other) => Err(anyhow!("WebappSwitchTo failed: {other:?}")),
  }
}
