//! credit to the librespot project

use std::{
  sync::{Arc, Mutex, RwLock},
  time::Duration,
};

use reqwest::header::HeaderMap;
use tokio::sync::oneshot;

use crate::{
  error::{Error, Result},
  http::{CLIENT_VERSION, HTTP_CONNECT_TIMEOUT, HTTP_REQUEST_TIMEOUT},
};

#[derive(Debug, Clone, Copy, uniffi::Enum)]
pub enum HttpMethod {
  Get,
  Post,
  Put,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct HttpHeader {
  pub name: String,
  pub value: String,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct HttpRequest {
  pub method: HttpMethod,
  pub url: String,
  pub headers: Vec<HttpHeader>,
  pub body: Vec<u8>,
  pub timeout_ms: u32,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct HttpResponse {
  pub status: u16,
  pub headers: Vec<HttpHeader>,
  pub body: Vec<u8>,
}

impl HttpResponse {
  pub(crate) fn ok(&self) -> bool {
    (200..300).contains(&self.status)
  }

  pub(crate) fn text(&self) -> String {
    String::from_utf8_lossy(&self.body).into_owned()
  }
}

#[derive(uniffi::Object)]
pub struct HttpSink {
  tx: Mutex<Option<oneshot::Sender<std::result::Result<HttpResponse, String>>>>,
}

#[uniffi::export]
impl HttpSink {
  pub fn complete(&self, response: HttpResponse) {
    if let Some(tx) = self.tx.lock().unwrap().take() {
      let _ = tx.send(Ok(response));
    }
  }

  pub fn fail(&self, reason: String) {
    if let Some(tx) = self.tx.lock().unwrap().take() {
      let _ = tx.send(Err(reason));
    }
  }
}

#[uniffi::export(with_foreign)]
pub trait HttpTransport: Send + Sync {
  fn execute(&self, request: HttpRequest, sink: Arc<HttpSink>);
}

#[derive(Clone)]
pub struct HttpExecutor {
  transport: Arc<RwLock<Arc<dyn HttpTransport>>>,
}

impl HttpExecutor {
  pub fn new() -> Self {
    HttpExecutor {
      transport: Arc::new(RwLock::new(Arc::new(ReqwestTransport::new()))),
    }
  }

  pub fn set(&self, transport: Arc<dyn HttpTransport>) {
    *self.transport.write().unwrap() = transport;
  }

  pub async fn execute(&self, request: HttpRequest) -> Result<HttpResponse> {
    let transport = self.transport.read().unwrap().clone();
    let (tx, rx) = oneshot::channel();
    let sink = Arc::new(HttpSink { tx: Mutex::new(Some(tx)) });
    tracing::trace!(method = ?request.method, url = %request.url, bytes = request.body.len(), "http request");
    transport.execute(request, sink);
    match rx.await {
      Ok(Ok(resp)) => {
        tracing::debug!(status = resp.status, bytes = resp.body.len(), "http response");
        Ok(resp)
      }
      Ok(Err(reason)) => {
        tracing::warn!(%reason, "http transport error");
        Err(Error::other(reason))
      }
      Err(_) => {
        tracing::warn!("http transport dropped without responding");
        Err(Error::other("http transport dropped without responding"))
      }
    }
  }
}

impl Default for HttpExecutor {
  fn default() -> Self {
    Self::new()
  }
}

pub struct ReqwestTransport {
  client: reqwest::Client,
}

impl ReqwestTransport {
  pub fn new() -> Self {
    let client = reqwest::Client::builder()
      .user_agent(format!("Spotify/{CLIENT_VERSION} Android/36 (SM-X810)"))
      .timeout(HTTP_REQUEST_TIMEOUT)
      .connect_timeout(HTTP_CONNECT_TIMEOUT)
      .build()
      .expect("reqwest client builds");
    ReqwestTransport { client }
  }
}

impl Default for ReqwestTransport {
  fn default() -> Self {
    Self::new()
  }
}

impl HttpTransport for ReqwestTransport {
  fn execute(&self, request: HttpRequest, sink: Arc<HttpSink>) {
    let client = self.client.clone();
    tokio::spawn(async move {
      match reqwest_execute(&client, request).await {
        Ok(resp) => sink.complete(resp),
        Err(e) => sink.fail(e.to_string()),
      }
    });
  }
}

async fn reqwest_execute(client: &reqwest::Client, request: HttpRequest) -> Result<HttpResponse> {
  let method = match request.method {
    HttpMethod::Get => reqwest::Method::GET,
    HttpMethod::Post => reqwest::Method::POST,
    HttpMethod::Put => reqwest::Method::PUT,
  };
  let mut rb = client.request(method, request.url.as_str());
  for h in &request.headers {
    rb = rb.header(h.name.as_str(), h.value.as_str());
  }
  if request.timeout_ms > 0 {
    rb = rb.timeout(Duration::from_millis(request.timeout_ms as u64));
  }
  if !request.body.is_empty() {
    rb = rb.body(request.body);
  }
  let resp = rb.send().await?;
  let status = resp.status().as_u16();
  let headers = header_vec(resp.headers());
  let body = resp.bytes().await?.to_vec();
  Ok(HttpResponse { status, headers, body })
}

fn header_vec(map: &HeaderMap) -> Vec<HttpHeader> {
  map
    .iter()
    .map(|(k, v)| HttpHeader {
      name: k.as_str().to_string(),
      value: v.to_str().unwrap_or_default().to_string(),
    })
    .collect()
}

pub(crate) fn headers_to_vec(map: &HeaderMap) -> Vec<HttpHeader> {
  header_vec(map)
}

pub(crate) fn with_query(base: String, query: &[(&str, String)]) -> Result<String> {
  if query.is_empty() {
    return Ok(base);
  }
  let url = url::Url::parse_with_params(&base, query.iter().map(|(k, v)| (*k, v.as_str())))?;
  Ok(url.to_string())
}

pub(crate) fn form_urlencode(pairs: &[(&str, &str)]) -> Vec<u8> {
  let mut ser = url::form_urlencoded::Serializer::new(String::new());
  for (k, v) in pairs {
    ser.append_pair(k, v);
  }
  ser.finish().into_bytes()
}
