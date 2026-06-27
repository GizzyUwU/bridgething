//! credit to the librespot project

use std::time::{Duration, Instant};

use serde::Deserialize;
use tokio::sync::Mutex;

use crate::error::{Error, Result};

pub const DEFAULT_WORKER_BASE: &str = "https://thinglabs.sh/auth";
pub const DEFAULT_SCOPE: &str = "streaming,user-read-playback-state,user-modify-playback-state,\
user-library-read,user-library-modify,user-read-private,user-follow-read,\
user-read-recently-played,playlist-read-private,playlist-read-collaborative";
const DEVICE_GRANT: &str = "urn:ietf:params:oauth:grant-type:device_code";

#[uniffi::export(callback_interface)]
pub trait TokenStore: Send + Sync {
  fn load_refresh_token(&self) -> Option<String>;
  fn save_refresh_token(&self, token: String);
  fn load_username(&self) -> Option<String>;
  fn save_username(&self, username: String);
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct DeviceFlow {
  pub device_code: String,
  pub user_code: String,
  pub verification_uri: String,
  pub interval: u64,
  pub expires_in: u64,
}

struct BearerState {
  refresh_token: Option<String>,
  bearer: Option<String>,
  bearer_exp: Instant,
}

pub struct Auth {
  base: String,
  psk: String,
  http: reqwest::Client,
  store: Box<dyn TokenStore>,
  state: Mutex<BearerState>,
}

#[derive(Deserialize)]
struct DeviceCodeResp {
  device_code: String,
  user_code: String,
  verification_url: Option<String>,
  verification_url_prefilled: Option<String>,
  #[serde(default = "default_interval")]
  interval: u64,
  #[serde(default = "default_expires")]
  expires_in: u64,
}

fn default_interval() -> u64 {
  5
}
fn default_expires() -> u64 {
  600
}

#[derive(Deserialize)]
struct TokenResp {
  access_token: Option<String>,
  refresh_token: Option<String>,
  #[serde(default = "default_token_ttl")]
  expires_in: u64,
  error: Option<String>,
}

fn default_token_ttl() -> u64 {
  3600
}

impl Auth {
  pub fn new(base: impl Into<String>, psk: impl Into<String>, store: Box<dyn TokenStore>) -> Self {
    Auth {
      base: base.into().trim_end_matches('/').to_string(),
      psk: psk.into(),
      http: reqwest::Client::builder()
        .timeout(crate::http::HTTP_REQUEST_TIMEOUT)
        .connect_timeout(crate::http::HTTP_CONNECT_TIMEOUT)
        .build()
        .expect("reqwest client builds"),
      store,
      state: Mutex::new(BearerState {
        refresh_token: None,
        bearer: None,
        bearer_exp: Instant::now(),
      }),
    }
  }

  pub fn store(&self) -> &dyn TokenStore {
    &*self.store
  }

  pub async fn is_paired(&self) -> bool {
    if self.state.lock().await.refresh_token.is_some() {
      return true;
    }
    if let Some(rt) = self.store.load_refresh_token() {
      self.state.lock().await.refresh_token = Some(rt);
      return true;
    }
    false
  }

  fn worker_post(&self, path: &str) -> reqwest::RequestBuilder {
    self
      .http
      .post(format!("{}{}", self.base, path))
      .header(reqwest::header::AUTHORIZATION, format!("Bearer {}", self.psk))
  }

  pub async fn begin_device_flow(&self) -> Result<DeviceFlow> {
    let resp = self
      .worker_post("/api/device/code")
      .form(&[("scope", DEFAULT_SCOPE), ("description", "bridgething-carthing")])
      .send()
      .await?;
    let status = resp.status();
    let text = resp.text().await?;
    if !status.is_success() {
      return Err(Error::status("device/code", status.as_u16(), text));
    }
    let dc: DeviceCodeResp = serde_json::from_str(&text)?;
    Ok(DeviceFlow {
      verification_uri: dc
        .verification_url_prefilled
        .or(dc.verification_url)
        .unwrap_or_default(),
      user_code: dc.user_code,
      device_code: dc.device_code,
      interval: dc.interval,
      expires_in: dc.expires_in,
    })
  }

  pub async fn complete_device_flow(&self, flow: &DeviceFlow) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(flow.expires_in);
    let mut interval = flow.interval.max(1);
    loop {
      if Instant::now() >= deadline {
        return Err(Error::PairingTimeout);
      }
      tokio::time::sleep(Duration::from_secs(interval)).await;
      let resp = match self
        .worker_post("/api/token")
        .form(&[("grant_type", DEVICE_GRANT), ("device_code", &flow.device_code)])
        .send()
        .await
      {
        Ok(r) => r,
        Err(e) => {
          tracing::warn!("device-flow poll send failed: {e}");
          continue;
        }
      };
      let status = resp.status();
      let body = match resp.text().await {
        Ok(t) => t,
        Err(e) => {
          tracing::warn!("device-flow poll read failed: {e}");
          continue;
        }
      };
      let tok: TokenResp = match serde_json::from_str(&body) {
        Ok(t) => t,
        Err(e) => {
          tracing::warn!("device-flow poll parse failed: {e}");
          continue;
        }
      };
      if status.is_success()
        && let (Some(access), Some(refresh)) = (tok.access_token, tok.refresh_token)
      {
        self.adopt(refresh, access, tok.expires_in).await;
        return Ok(());
      }
      match tok.error.as_deref() {
        Some("authorization_pending") => {}
        Some("slow_down") => interval += 2,
        Some(other) => return Err(Error::Auth(other.to_string())),
        None => {}
      }
    }
  }

  async fn adopt(&self, refresh: String, bearer: String, ttl: u64) {
    self.store.save_refresh_token(refresh.clone());
    let mut st = self.state.lock().await;
    st.refresh_token = Some(refresh);
    st.bearer = Some(bearer);
    st.bearer_exp = Instant::now() + Duration::from_secs(ttl);
  }

  pub async fn bearer(&self) -> Result<String> {
    let mut st = self.state.lock().await;
    if let Some(b) = &st.bearer
      && Instant::now() + Duration::from_secs(60) < st.bearer_exp
    {
      return Ok(b.clone());
    }
    let refresh = match st.refresh_token.clone() {
      Some(rt) => rt,
      None => match self.store.load_refresh_token() {
        Some(rt) => {
          st.refresh_token = Some(rt.clone());
          rt
        }
        None => return Err(Error::NotPaired),
      },
    };
    let resp = self
      .worker_post("/api/token")
      .form(&[("grant_type", "refresh_token"), ("refresh_token", &refresh)])
      .send()
      .await?;
    let status = resp.status();
    let text = resp.text().await?;
    if status == reqwest::StatusCode::BAD_REQUEST {
      return Err(Error::InvalidGrant);
    }
    if !status.is_success() {
      return Err(Error::status("token/refresh", status.as_u16(), text));
    }
    let tok: TokenResp = serde_json::from_str(&text)?;
    let bearer = tok.access_token.ok_or(Error::InvalidGrant)?;
    if let Some(new_rt) = tok.refresh_token
      && new_rt != refresh
    {
      self.store.save_refresh_token(new_rt.clone());
      st.refresh_token = Some(new_rt);
    }
    st.bearer = Some(bearer.clone());
    st.bearer_exp = Instant::now() + Duration::from_secs(tok.expires_in);
    Ok(bearer)
  }
}
