//! credit to the librespot project

use std::time::{Duration, Instant};

use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue};
use serde::Deserialize;
use tokio::sync::Mutex;

use crate::{
  error::{Error, Result},
  httpx::{HttpExecutor, HttpMethod, HttpRequest, HttpResponse, form_urlencode, headers_to_vec},
};

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
  exec: HttpExecutor,
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
  pub fn new(base: impl Into<String>, psk: impl Into<String>, store: Box<dyn TokenStore>, exec: HttpExecutor) -> Self {
    Auth {
      base: base.into().trim_end_matches('/').to_string(),
      psk: psk.into(),
      exec,
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

  async fn worker_form(&self, path: &str, pairs: &[(&str, &str)]) -> Result<HttpResponse> {
    let mut headers = HeaderMap::new();
    headers.insert(
      AUTHORIZATION,
      HeaderValue::from_str(&format!("Bearer {}", self.psk)).map_err(Error::other)?,
    );
    headers.insert(
      CONTENT_TYPE,
      HeaderValue::from_static("application/x-www-form-urlencoded"),
    );
    self
      .exec
      .execute(HttpRequest {
        method: HttpMethod::Post,
        url: format!("{}{}", self.base, path),
        headers: headers_to_vec(&headers),
        body: form_urlencode(pairs),
        timeout_ms: 0,
      })
      .await
  }

  pub async fn begin_device_flow(&self) -> Result<DeviceFlow> {
    let resp = self
      .worker_form(
        "/api/device/code",
        &[("scope", DEFAULT_SCOPE), ("description", "bridgething-carthing")],
      )
      .await?;
    let text = resp.text();
    if !resp.ok() {
      return Err(Error::status("device/code", resp.status, text));
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
        .worker_form("/api/token", &[("grant_type", DEVICE_GRANT), ("device_code", &flow.device_code)])
        .await
      {
        Ok(r) => r,
        Err(e) => {
          tracing::warn!("device-flow poll send failed: {e}");
          continue;
        }
      };
      let status = resp.status;
      let body = resp.text();
      let tok: TokenResp = match serde_json::from_str(&body) {
        Ok(t) => t,
        Err(e) => {
          tracing::warn!("device-flow poll parse failed: {e}");
          continue;
        }
      };
      if (200..300).contains(&status)
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
    tracing::debug!(ttl_s = ttl, "auth: adopting new tokens");
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
    tracing::debug!("auth: bearer expired, refreshing");
    let resp = self
      .worker_form("/api/token", &[("grant_type", "refresh_token"), ("refresh_token", &refresh)])
      .await?;
    let status = resp.status;
    let text = resp.text();
    if status == 400 {
      return Err(Error::InvalidGrant);
    }
    if !resp.ok() {
      return Err(Error::status("token/refresh", status, text));
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
    tracing::debug!(ttl_s = tok.expires_in, "auth: bearer refreshed");
    Ok(bearer)
  }
}
