use std::{
  sync::{Arc, Mutex as StdMutex},
  time::{Duration, Instant},
};

use ::http::header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue};
use bridgething_io::{HttpExecutor, HttpMethod, HttpRequest, HttpResponse};
use serde::Deserialize;
use tokio::sync::{Mutex, watch};

use crate::{
  error::{Error, Result},
  httpx::{form_urlencode, headers_to_vec},
};

pub const DEFAULT_WORKER_BASE: &str = "https://thinglabs.sh/auth";
pub const DEFAULT_SCOPE: &str = "streaming,user-read-playback-state,user-modify-playback-state,\
user-library-read,user-library-modify,user-read-private,user-follow-read,\
user-read-recently-played,playlist-read-private,playlist-read-collaborative";
const DEVICE_GRANT: &str = "urn:ietf:params:oauth:grant-type:device_code";
const BEARER_SKEW: Duration = Duration::from_secs(60);
const MAX_TTL_SECONDS: u64 = 24 * 60 * 60;

/// clamped because `Instant + Duration` panics on overflow and the ttl is whatever the worker said.
fn expiry(ttl_seconds: u64) -> Instant {
  Instant::now() + Duration::from_secs(ttl_seconds.min(MAX_TTL_SECONDS))
}

pub trait TokenStore: Send + Sync {
  fn load_refresh_token(&self) -> Option<String>;
  fn save_refresh_token(&self, token: String);
  fn load_username(&self) -> Option<String>;
  fn save_username(&self, username: String);
}

#[derive(Debug, Clone)]
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

#[derive(Clone)]
enum Refreshed {
  Bearer(String),
  InvalidGrant,
  NotPaired,
  Failed(String),
}

/// clears the in-flight slot however the refresh task ends, so a panicking refresh cannot leave every
/// later caller waiting on a receiver whose sender is already gone.
struct ClearInflight(Arc<Auth>);

impl Drop for ClearInflight {
  fn drop(&mut self) {
    let mut held = self
      .0
      .refreshing
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner());
    *held = None;
  }
}

#[derive(Clone)]
enum Paired {
  Adopted,
  Timeout,
  Failed(String),
}

pub struct Auth {
  base: String,
  psk: String,
  exec: HttpExecutor,
  store: Box<dyn TokenStore>,
  state: Mutex<BearerState>,
  refreshing: StdMutex<Option<watch::Receiver<Option<Refreshed>>>>,
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
      refreshing: StdMutex::new(None),
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
      .map_err(Error::other)
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

  /// polls on a detached task because the grant is spent the moment the worker answers, so a caller that
  /// goes away between that answer and the write would strand the pairing.
  pub async fn complete_device_flow(self: &Arc<Self>, flow: &DeviceFlow) -> Result<()> {
    let (tx, mut rx) = watch::channel(None);
    let auth = self.clone();
    let flow = flow.clone();
    tokio::spawn(async move {
      let _ = tx.send(Some(auth.poll_device_flow(flow).await));
    });
    loop {
      if let Some(outcome) = rx.borrow_and_update().clone() {
        return match outcome {
          Paired::Adopted => Ok(()),
          Paired::Timeout => Err(Error::PairingTimeout),
          Paired::Failed(reason) => Err(Error::Auth(reason)),
        };
      }
      rx.changed()
        .await
        .map_err(|_| Error::other("device flow ended without a result"))?;
    }
  }

  async fn poll_device_flow(&self, flow: DeviceFlow) -> Paired {
    let deadline = expiry(flow.expires_in);
    let mut interval = flow.interval.max(1);
    loop {
      if Instant::now() >= deadline {
        return Paired::Timeout;
      }
      tokio::time::sleep(Duration::from_secs(interval)).await;
      let resp = match self
        .worker_form(
          "/api/token",
          &[("grant_type", DEVICE_GRANT), ("device_code", &flow.device_code)],
        )
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
        return Paired::Adopted;
      }
      match tok.error.as_deref() {
        Some("authorization_pending") => {}
        Some("slow_down") => interval += 2,
        Some(other) => return Paired::Failed(other.to_string()),
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
    st.bearer_exp = expiry(ttl);
  }

  /// the refresh runs on a detached task because spotify kills the presented refresh token the moment the
  /// request lands, so the rotated successor has to be persisted whether or not a caller is still waiting.
  pub async fn bearer(self: &Arc<Self>) -> Result<String> {
    if let Some(bearer) = self.live_bearer().await {
      return Ok(bearer);
    }
    let mut rx = self.start_refresh();
    loop {
      if let Some(outcome) = rx.borrow_and_update().clone() {
        return match outcome {
          Refreshed::Bearer(bearer) => Ok(bearer),
          Refreshed::InvalidGrant => Err(Error::InvalidGrant),
          Refreshed::NotPaired => Err(Error::NotPaired),
          Refreshed::Failed(reason) => Err(Error::other(reason)),
        };
      }
      rx.changed()
        .await
        .map_err(|_| Error::other("token refresh ended without a result"))?;
    }
  }

  async fn live_bearer(&self) -> Option<String> {
    let st = self.state.lock().await;
    st.bearer
      .clone()
      .filter(|_| Instant::now() + BEARER_SKEW < st.bearer_exp)
  }

  /// takes only `refreshing`, never `state`, so there is no lock pair here to order against the refresh
  /// task's own `state` use.
  fn start_refresh(self: &Arc<Self>) -> watch::Receiver<Option<Refreshed>> {
    let mut inflight = self.refreshing.lock().unwrap();
    if let Some(rx) = inflight.as_ref() {
      return rx.clone();
    }
    let (tx, rx) = watch::channel(None);
    *inflight = Some(rx.clone());
    drop(inflight);

    let auth = self.clone();
    tokio::spawn(async move {
      let clear = ClearInflight(auth.clone());
      let outcome = auth.refresh_now().await;
      drop(clear);
      let _ = tx.send(Some(outcome));
    });
    rx
  }

  async fn refresh_now(&self) -> Refreshed {
    let refresh = {
      let mut st = self.state.lock().await;
      match st.refresh_token.clone() {
        Some(rt) => rt,
        None => match self.store.load_refresh_token() {
          Some(rt) => {
            st.refresh_token = Some(rt.clone());
            rt
          }
          None => return Refreshed::NotPaired,
        },
      }
    };
    tracing::debug!("auth: bearer expired, refreshing");
    let resp = match self
      .worker_form(
        "/api/token",
        &[("grant_type", "refresh_token"), ("refresh_token", &refresh)],
      )
      .await
    {
      Ok(resp) => resp,
      Err(e) => return Refreshed::Failed(e.to_string()),
    };
    let status = resp.status;
    let text = resp.text();
    if status == 400 {
      return Refreshed::InvalidGrant;
    }
    if !resp.ok() {
      return Refreshed::Failed(Error::status("token/refresh", status, text).to_string());
    }
    let tok: TokenResp = match serde_json::from_str(&text) {
      Ok(tok) => tok,
      Err(e) => return Refreshed::Failed(e.to_string()),
    };
    let Some(bearer) = tok.access_token else {
      return Refreshed::InvalidGrant;
    };
    let mut st = self.state.lock().await;
    if let Some(new_rt) = tok.refresh_token
      && new_rt != refresh
    {
      self.store.save_refresh_token(new_rt.clone());
      st.refresh_token = Some(new_rt);
    }
    st.bearer = Some(bearer.clone());
    st.bearer_exp = expiry(tok.expires_in);
    tracing::debug!(ttl_s = tok.expires_in, "auth: bearer refreshed");
    Refreshed::Bearer(bearer)
  }
}

#[cfg(test)]
mod tests {
  use std::sync::{
    Mutex as StdMutex,
    atomic::{AtomicBool, AtomicUsize, Ordering},
  };

  use bridgething_io::{HttpDownloadSink, HttpSink, HttpTransport};

  use super::*;

  #[derive(Clone, Default)]
  struct MemoryStore {
    refresh: Arc<StdMutex<Option<String>>>,
    fail_next_save: Arc<AtomicBool>,
  }

  impl MemoryStore {
    /// stands in for a foreign keychain that throws: the refresh task dies mid-flight without sending.
    fn fail_next_save(&self) {
      self.fail_next_save.store(true, Ordering::SeqCst);
    }
  }

  impl TokenStore for MemoryStore {
    fn load_refresh_token(&self) -> Option<String> {
      self.refresh.lock().unwrap().clone()
    }
    fn save_refresh_token(&self, token: String) {
      if self.fail_next_save.swap(false, Ordering::SeqCst) {
        panic!("the secret store refused the write");
      }
      *self.refresh.lock().unwrap() = Some(token);
    }
    fn load_username(&self) -> Option<String> {
      None
    }
    fn save_username(&self, _username: String) {}
  }

  /// mirrors spotify's rotation: the live refresh token issues a successor and dies, anything else is
  /// invalid_grant.
  struct RotatingWorker {
    live: Arc<StdMutex<String>>,
    presented: Arc<StdMutex<Vec<String>>>,
    issued: Arc<AtomicUsize>,
    latency: Duration,
  }

  impl HttpTransport for RotatingWorker {
    fn execute(&self, request: HttpRequest, sink: Arc<HttpSink>) {
      let presented = form_value(&request.body, "refresh_token");
      self.presented.lock().unwrap().push(presented.clone());
      let rotated = {
        let mut live = self.live.lock().unwrap();
        (*live == presented).then(|| {
          let next = format!("rt-{}", self.issued.fetch_add(1, Ordering::SeqCst) + 1);
          *live = next.clone();
          next
        })
      };
      let latency = self.latency;
      tokio::spawn(async move {
        tokio::time::sleep(latency).await;
        match rotated {
          Some(next) => sink.complete(HttpResponse {
            status: 200,
            headers: Vec::new(),
            body: format!(r#"{{"access_token":"bearer-{next}","refresh_token":"{next}","expires_in":3600}}"#)
              .into_bytes(),
          }),
          None => sink.complete(HttpResponse {
            status: 400,
            headers: Vec::new(),
            body: br#"{"error":"invalid_grant"}"#.to_vec(),
          }),
        }
      });
    }

    fn download(&self, _request: HttpRequest, sink: Arc<HttpDownloadSink>) {
      sink.on_failed("the test transport has no streaming arm".to_string());
    }
  }

  fn form_value(body: &[u8], key: &str) -> String {
    String::from_utf8_lossy(body)
      .split('&')
      .find_map(|pair| pair.strip_prefix(&format!("{key}=")))
      .unwrap_or_default()
      .to_string()
  }

  struct Rig {
    auth: Arc<Auth>,
    store: MemoryStore,
    presented: Arc<StdMutex<Vec<String>>>,
  }

  fn rig(latency: Duration) -> Rig {
    let store = MemoryStore::default();
    store.save_refresh_token("rt-0".to_string());
    let presented = Arc::new(StdMutex::new(Vec::new()));
    let worker = RotatingWorker {
      live: Arc::new(StdMutex::new("rt-0".to_string())),
      presented: presented.clone(),
      issued: Arc::new(AtomicUsize::new(0)),
      latency,
    };
    let auth = Arc::new(Auth::new(
      "https://worker.test/auth",
      "psk",
      Box::new(store.clone()),
      HttpExecutor::new(Arc::new(worker)),
    ));
    Rig { auth, store, presented }
  }

  /// answers the device-code poll with an approved grant, so the tokens only survive if the poll is not
  /// riding the caller's future.
  struct ApprovingWorker {
    polls: Arc<AtomicUsize>,
    expires_in: u64,
  }

  impl HttpTransport for ApprovingWorker {
    fn execute(&self, _request: HttpRequest, sink: Arc<HttpSink>) {
      self.polls.fetch_add(1, Ordering::SeqCst);
      sink.complete(HttpResponse {
        status: 200,
        headers: Vec::new(),
        body: format!(
          r#"{{"access_token":"granted-bearer","refresh_token":"granted-refresh","expires_in":{}}}"#,
          self.expires_in
        )
        .into_bytes(),
      });
    }

    fn download(&self, _request: HttpRequest, sink: Arc<HttpDownloadSink>) {
      sink.on_failed("the test transport has no streaming arm".to_string());
    }
  }

  #[tokio::test]
  async fn an_abandoned_device_flow_still_persists_the_granted_tokens() {
    let store = MemoryStore::default();
    let polls = Arc::new(AtomicUsize::new(0));
    let auth = Arc::new(Auth::new(
      "https://worker.test/auth",
      "psk",
      Box::new(store.clone()),
      HttpExecutor::new(Arc::new(ApprovingWorker {
        polls: polls.clone(),
        expires_in: 3600,
      })),
    ));
    let flow = DeviceFlow {
      device_code: "dc-1".to_string(),
      user_code: "ABCD".to_string(),
      verification_uri: "https://worker.test/activate".to_string(),
      interval: 1,
      expires_in: 600,
    };

    let pairing = auth.clone();
    let abandoned = tokio::spawn(async move { pairing.complete_device_flow(&flow).await });
    tokio::time::sleep(Duration::from_millis(50)).await;
    abandoned.abort();

    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert_eq!(polls.load(Ordering::SeqCst), 1, "the poll outlived its caller");
    assert_eq!(store.load_refresh_token().as_deref(), Some("granted-refresh"));
  }

  #[tokio::test]
  async fn an_abandoned_refresh_still_persists_the_rotated_token() {
    let rig = rig(Duration::from_millis(80));

    let auth = rig.auth.clone();
    let abandoned = tokio::spawn(async move { auth.bearer().await });
    tokio::time::sleep(Duration::from_millis(20)).await;
    abandoned.abort();

    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(rig.store.load_refresh_token().as_deref(), Some("rt-1"));

    let bearer = rig.auth.bearer().await.expect("the rotated token is still good");
    assert_eq!(bearer, "bearer-rt-1");
    assert_eq!(*rig.presented.lock().unwrap(), vec!["rt-0".to_string()]);
  }

  #[tokio::test]
  async fn concurrent_callers_share_one_refresh() {
    let rig = rig(Duration::from_millis(40));

    let (left, right) = tokio::join!(rig.auth.bearer(), rig.auth.bearer());
    assert_eq!(left.unwrap(), "bearer-rt-1");
    assert_eq!(right.unwrap(), "bearer-rt-1");
    assert_eq!(*rig.presented.lock().unwrap(), vec!["rt-0".to_string()]);
    assert_eq!(rig.store.load_refresh_token().as_deref(), Some("rt-1"));
  }

  #[tokio::test]
  async fn a_refresh_that_dies_does_not_wedge_the_next_one() {
    let rig = rig(Duration::from_millis(0));
    rig.store.fail_next_save();

    assert!(rig.auth.bearer().await.is_err(), "the caller sees the refresh die");

    // the rotation really is gone once the write throws, so the verdict here is spotify's, not a stale
    // channel's: what matters is that a second request went out at all.
    let after = rig.auth.bearer().await;
    assert!(matches!(after, Err(Error::InvalidGrant)), "got {after:?}");
    assert_eq!(
      rig.presented.lock().unwrap().len(),
      2,
      "the next caller ran its own refresh instead of joining the dead one"
    );
  }

  #[tokio::test]
  async fn a_garbage_expiry_does_not_panic_the_refresh() {
    let store = MemoryStore::default();
    store.save_refresh_token("rt-0".to_string());
    let auth = Arc::new(Auth::new(
      "https://worker.test/auth",
      "psk",
      Box::new(store.clone()),
      HttpExecutor::new(Arc::new(ApprovingWorker {
        polls: Arc::new(AtomicUsize::new(0)),
        expires_in: u64::MAX,
      })),
    ));

    let bearer = auth.bearer().await.expect("an absurd ttl is clamped, not fatal");
    assert_eq!(bearer, "granted-bearer");
    assert_eq!(
      auth.bearer().await.expect("and the next call still works"),
      "granted-bearer"
    );
  }

  #[tokio::test]
  async fn a_dead_refresh_token_surfaces_as_invalid_grant() {
    let rig = rig(Duration::from_millis(0));
    rig.store.save_refresh_token("rt-stale".to_string());

    let failure = rig.auth.bearer().await.expect_err("a consumed token is rejected");
    assert!(matches!(failure, Error::InvalidGrant), "got {failure:?}");
  }
}
