//! credit to the librespot project

use std::{
  sync::Arc,
  time::{Duration, Instant},
};

use reqwest::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue};
use tokio::sync::Mutex;

use crate::{
  auth::Auth,
  error::{Error, Result},
  util,
};

pub const SPCLIENT: &str = "https://guc3-spclient.spotify.com";
pub const CLIENT_VERSION: &str = "9.1.52.1394";

pub(crate) const HTTP_REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
pub(crate) const HTTP_CONNECT_TIMEOUT: Duration = Duration::from_secs(8);
pub const ANDROID_CLIENT_ID: &str = "9a8d2f0ce77a4e248bb71fefcb557637";
pub const PROTO_CT: &str = "application/x-protobuf";

const DESKTOP_CLIENT_ID: &str = "65b708073fc0480ea92a077233ca87bd";
const CT_CLIENT_VERSION: &str = "1.2.52.442.g01a57f5f";
const CLIENTTOKEN_URL: &str = "https://clienttoken.spotify.com/v1/clienttoken";

pub fn random_hex(n: usize) -> String {
  let bytes: Vec<u8> = (0..n).map(|_| rand::random::<u8>()).collect();
  hex::encode(bytes)
}

struct ClientToken {
  device_id: String,
  token: Option<String>,
  exp: Instant,
  disabled: bool,
}

#[derive(Clone)]
pub struct SpHttp {
  pub http: reqwest::Client,
  pub auth: Arc<Auth>,
  ct: Arc<Mutex<ClientToken>>,
  market: Arc<Mutex<Option<(String, String)>>>,
}

impl SpHttp {
  pub fn new(auth: Arc<Auth>) -> Self {
    let http = reqwest::Client::builder()
      .user_agent(format!("Spotify/{CLIENT_VERSION} Android/36 (SM-X810)"))
      .timeout(HTTP_REQUEST_TIMEOUT)
      .connect_timeout(HTTP_CONNECT_TIMEOUT)
      .build()
      .expect("reqwest client builds");
    SpHttp {
      http,
      auth,
      ct: Arc::new(Mutex::new(ClientToken {
        device_id: random_hex(20),
        token: None,
        exp: Instant::now(),
        disabled: false,
      })),
      market: Arc::new(Mutex::new(None)),
    }
  }

  pub async fn set_market(&self, country: &str, catalogue: &str) {
    if !country.is_empty() && !catalogue.is_empty() {
      *self.market.lock().await = Some((country.to_string(), catalogue.to_string()));
    }
  }

  pub async fn market(&self) -> (String, String) {
    self
      .market
      .lock()
      .await
      .clone()
      .unwrap_or_else(|| ("US".to_string(), "premium".to_string()))
  }

  pub async fn headers(&self, json: bool) -> Result<HeaderMap> {
    let bearer = self.auth.bearer().await?;
    let mut h = HeaderMap::new();
    h.insert(
      AUTHORIZATION,
      HeaderValue::from_str(&format!("Bearer {bearer}")).map_err(Error::other)?,
    );
    h.insert("App-Platform", HeaderValue::from_static("Android"));
    h.insert("Spotify-App-Version", HeaderValue::from_static(CLIENT_VERSION));
    h.insert(
      ACCEPT,
      HeaderValue::from_static(if json { "application/json" } else { PROTO_CT }),
    );
    h.insert(CONTENT_TYPE, HeaderValue::from_static(PROTO_CT));
    if let Some(tok) = self.client_token().await
      && let Ok(v) = HeaderValue::from_str(&tok)
    {
      h.insert("client-token", v);
    }
    Ok(h)
  }

  async fn client_token(&self) -> Option<String> {
    let mut st = self.ct.lock().await;
    if st.disabled {
      return None;
    }
    if let Some(t) = &st.token
      && Instant::now() + Duration::from_secs(3600) < st.exp
    {
      return Some(t.clone());
    }
    match self.mint_client_token(&st.device_id).await {
      Some((tok, ttl)) => {
        st.token = Some(tok.clone());
        st.exp = Instant::now() + Duration::from_secs(ttl);
        Some(tok)
      }
      None => {
        st.disabled = true;
        None
      }
    }
  }

  async fn mint_client_token(&self, device_id: &str) -> Option<(String, u64)> {
    let body = util::client_token_request(CT_CLIENT_VERSION, DESKTOP_CLIENT_ID, device_id);
    let resp = self
      .http
      .post(CLIENTTOKEN_URL)
      .header(ACCEPT, PROTO_CT)
      .header(CONTENT_TYPE, PROTO_CT)
      .timeout(Duration::from_secs(5))
      .body(body)
      .send()
      .await
      .ok()?;
    if !resp.status().is_success() {
      return None;
    }
    let bytes = resp.bytes().await.ok()?;
    util::parse_client_token(&bytes)
  }
}
