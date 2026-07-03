//! credit to the librespot project

use std::{
  io::Read,
  sync::{Arc, Mutex},
  time::Duration,
};

use base64::Engine;
use librespot_protocol::{
  connect::{
    Capabilities, Cluster, ClusterUpdate, ConnectLoggingParams, Device, DeviceInfo, MemberType, PutStateReason,
    PutStateRequest, SetVolumeCommand,
  },
  devices::DeviceType,
};
use protobuf::{Message, MessageField};
use reqwest::header::{CONTENT_TYPE, HeaderMap, HeaderValue};
use serde_json::{Value, json};
use tokio::sync::mpsc;

use crate::{
  error::{Error, Result},
  http::{ANDROID_CLIENT_ID, SPCLIENT, SpHttp, random_hex},
  httpx::{HttpMethod, with_query},
  model::LibraryScope,
  transport::{TungsteniteTransport, WsEvent, WsInbox, WsTransport},
  util::now_ms,
};

#[derive(Clone)]
pub struct Dealer {
  http: SpHttp,
  device_id: String,
  name: String,
  transport: Arc<Mutex<Arc<dyn WsTransport>>>,
}

impl Dealer {
  pub fn new(http: SpHttp, device_id: String) -> Self {
    Dealer {
      http,
      device_id,
      name: "bridgething".to_string(),
      transport: Arc::new(Mutex::new(Arc::new(TungsteniteTransport::new()))),
    }
  }

  pub fn set_transport(&self, transport: Arc<dyn WsTransport>) {
    *self.transport.lock().unwrap() = transport;
  }

  pub fn device_id(&self) -> &str {
    &self.device_id
  }

  async fn dealer_host(&self) -> Result<String> {
    let url = with_query(
      "https://apresolve.spotify.com/".to_string(),
      &[("type", "dealer".to_string())],
    )?;
    let resp = self
      .http
      .send(HttpMethod::Get, url, HeaderMap::new(), Vec::new(), 0)
      .await?;
    let v: Value = serde_json::from_slice(&resp.body)?;
    let host = v["dealer"][0]
      .as_str()
      .ok_or_else(|| Error::other("apresolve returned no dealer host"))?;
    Ok(host.split(':').next().unwrap_or(host).to_string())
  }

  pub async fn open(&self) -> Result<(DealerStream, DealerWriter)> {
    let host = self.dealer_host().await?;
    tracing::debug!(%host, "dealer: opening websocket");
    let bearer = self.http.auth.bearer().await?;
    let url = format!("wss://{host}/?access_token={bearer}");
    let transport = self.transport.lock().unwrap().clone();
    let (tx, mut rx) = mpsc::unbounded_channel::<WsEvent>();
    transport.connect(url, Arc::new(WsInbox::new(tx)));
    let connection_id = loop {
      match rx.recv().await {
        Some(WsEvent::Text(t)) => {
          let v: Value = serde_json::from_str(t.as_str())?;
          if let Some(cid) = v["headers"]["Spotify-Connection-Id"].as_str() {
            tracing::debug!(connection_id = %cid, "dealer: websocket connected");
            break cid.to_string();
          }
        }
        Some(WsEvent::Open) => {}
        Some(WsEvent::Closed(reason)) => {
          return Err(Error::other(format!("dealer closed before connection-id: {reason}")));
        }
        None => return Err(Error::other("dealer closed before connection-id")),
      }
    };
    let writer = DealerWriter {
      http: self.http.clone(),
      device_id: self.device_id.clone(),
      name: self.name.clone(),
      connection_id,
    };
    let mut ping = tokio::time::interval(DEALER_PING_INTERVAL);
    ping.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    Ok((
      DealerStream {
        rx,
        transport,
        ping,
        awaiting_response: false,
      },
      writer,
    ))
  }
}

const DEALER_PING_INTERVAL: Duration = Duration::from_secs(20);

pub enum DealerEvent {
  Cluster(Cluster),
  LibraryChanged(LibraryScope),
}

pub struct DealerStream {
  rx: mpsc::UnboundedReceiver<WsEvent>,
  transport: Arc<dyn WsTransport>,
  ping: tokio::time::Interval,
  awaiting_response: bool,
}

impl DealerStream {
  pub async fn next_event(&mut self) -> Result<Option<DealerEvent>> {
    loop {
      let event = tokio::select! {
        e = self.rx.recv() => e,
        _ = self.ping.tick() => {
          if self.awaiting_response {
            tracing::warn!("dealer: ping unanswered within interval; treating link as dead");
            return Ok(None);
          }
          tracing::debug!("dealer: ping");
          self.transport.send_text(r#"{"type":"ping"}"#.to_string());
          self.awaiting_response = true;
          continue;
        }
      };
      let Some(event) = event else { return Ok(None) };
      self.awaiting_response = false;
      let text = match event {
        WsEvent::Text(t) => t,
        WsEvent::Open => continue,
        WsEvent::Closed(_) => return Ok(None),
      };
      let msg: Value = match serde_json::from_str(text.as_str()) {
        Ok(v) => v,
        Err(_) => continue,
      };
      let kind = msg["type"].as_str();
      tracing::trace!(?kind, frame = %text, "dealer: raw frame");
      match kind {
        Some("ping") => {
          self.transport.send_text(r#"{"type":"pong"}"#.to_string());
        }
        Some("pong") => {}
        Some("request") => {
          if let Some(key) = msg["key"].as_str() {
            let reply = json!({"type": "reply", "key": key, "payload": {"success": true}});
            self.transport.send_text(reply.to_string());
          }
        }
        Some("message") => {
          let uri = msg["uri"].as_str().unwrap_or("");
          if uri.starts_with("hm://collection/") {
            tracing::debug!("dealer: library changed (saved)");
            return Ok(Some(DealerEvent::LibraryChanged(LibraryScope::Saved)));
          }
          if uri.starts_with("hm://playlist/") {
            tracing::debug!("dealer: library changed (playlists)");
            return Ok(Some(DealerEvent::LibraryChanged(LibraryScope::Playlists)));
          }
          if !uri.contains("connect-state/v1/cluster") {
            tracing::trace!(%uri, "dealer: ignoring non-cluster message");
            continue;
          }
          let gz = msg["headers"]["Transfer-Encoding"].as_str() == Some("gzip");
          if let Some(payloads) = msg["payloads"].as_array() {
            for p in payloads {
              if let Some(s) = p.as_str() {
                let raw = match decode_payload(s, gz) {
                  Ok(r) => r,
                  Err(e) => {
                    tracing::warn!(?e, "dealer: skipping undecodable payload");
                    continue;
                  }
                };
                let upd = match ClusterUpdate::parse_from_bytes(&raw) {
                  Ok(u) => u,
                  Err(e) => {
                    tracing::warn!(?e, "dealer: skipping unparseable cluster update");
                    continue;
                  }
                };
                if let Some(cluster) = upd.cluster.into_option() {
                  tracing::debug!(
                    active_device = %cluster.active_device_id,
                    track = %cluster.player_state.track.uri,
                    "dealer: cluster update"
                  );
                  return Ok(Some(DealerEvent::Cluster(cluster)));
                }
              }
            }
          }
        }
        _ => {}
      }
    }
  }
}

#[derive(Clone)]
pub struct DealerWriter {
  http: SpHttp,
  device_id: String,
  name: String,
  connection_id: String,
}

impl DealerWriter {
  pub fn connection_id(&self) -> &str {
    &self.connection_id
  }

  fn observer_put_state(&self) -> PutStateRequest {
    let mut caps = Capabilities::new();
    caps.can_be_player = false;
    caps.is_observable = true;
    caps.hidden = true;
    caps.needs_full_player_state = true;
    caps.volume_steps = 0;
    caps.supported_types.push("audio/track".to_string());

    let mut di = DeviceInfo::new();
    di.name = self.name.clone();
    di.device_id = self.device_id.clone();
    di.device_type = DeviceType::OBSERVER.into();
    di.client_id = ANDROID_CLIENT_ID.to_string();
    di.device_software_version = "bridgething-sfp/0.1".to_string();
    di.capabilities = MessageField::some(caps);

    let mut device = Device::new();
    device.device_info = MessageField::some(di);

    let mut req = PutStateRequest::new();
    req.device = MessageField::some(device);
    req.member_type = MemberType::CONNECT_STATE.into();
    req.is_active = false;
    req.put_state_reason = PutStateReason::NEW_DEVICE.into();
    req.client_side_timestamp = now_ms();
    req
  }

  pub async fn cluster(&self) -> Result<Cluster> {
    let body = self.observer_put_state().write_to_bytes()?;
    let mut headers = self.http.headers(false).await?;
    headers.insert(
      "X-Spotify-Connection-Id",
      HeaderValue::from_str(&self.connection_id).map_err(Error::other)?,
    );
    let url = format!("{SPCLIENT}/connect-state/v1/devices/{}", self.device_id);
    let resp = self.http.send(HttpMethod::Put, url, headers, body, 0).await?;
    if !resp.ok() {
      return Err(Error::status("get_cluster", resp.status, resp.text()));
    }
    Ok(Cluster::parse_from_bytes(&resp.body)?)
  }

  async fn player_command(&self, target: &str, command: Value) -> Result<(u16, String)> {
    let url = format!(
      "{SPCLIENT}/connect-state/v1/player/command/from/{}/to/{}",
      self.device_id, target
    );
    let mut headers = self.http.headers(true).await?;
    headers.insert(
      "X-Spotify-Connection-Id",
      HeaderValue::from_str(&self.connection_id).map_err(Error::other)?,
    );
    let endpoint = command["endpoint"].as_str().unwrap_or("?");
    tracing::debug!(%target, %endpoint, command = %command, "dealer: player command");
    let body = serde_json::to_vec(&json!({ "command": command }))?;
    let resp = self.http.send(HttpMethod::Post, url, headers, body, 0).await?;
    if !resp.ok() {
      return Err(Error::status("player_command", resp.status, resp.text()));
    }
    tracing::trace!(%endpoint, status = resp.status, "dealer: player command ok");
    Ok((resp.status, resp.text()))
  }

  pub async fn pause(&self, target: &str) -> Result<(u16, String)> {
    self.player_command(target, json!({"endpoint": "pause"})).await
  }
  pub async fn resume(&self, target: &str) -> Result<(u16, String)> {
    self.player_command(target, json!({"endpoint": "resume"})).await
  }
  pub async fn skip_next(&self, target: &str) -> Result<(u16, String)> {
    self.player_command(target, json!({"endpoint": "skip_next"})).await
  }
  pub async fn skip_prev(&self, target: &str) -> Result<(u16, String)> {
    self.player_command(target, json!({"endpoint": "skip_prev"})).await
  }
  pub async fn seek_to(&self, target: &str, ms: i64) -> Result<(u16, String)> {
    self
      .player_command(target, json!({"endpoint": "seek_to", "value": ms}))
      .await
  }
  pub async fn set_shuffle(&self, target: &str, on: bool) -> Result<(u16, String)> {
    self
      .player_command(target, json!({"endpoint": "set_shuffling_context", "value": on}))
      .await
  }
  pub async fn set_repeat_context(&self, target: &str, on: bool) -> Result<(u16, String)> {
    self
      .player_command(target, json!({"endpoint": "set_repeating_context", "value": on}))
      .await
  }
  pub async fn set_repeat_track(&self, target: &str, on: bool) -> Result<(u16, String)> {
    self
      .player_command(target, json!({"endpoint": "set_repeating_track", "value": on}))
      .await
  }

  pub async fn play(&self, target: &str, command: Value) -> Result<(u16, String)> {
    self.player_command(target, command).await
  }

  pub async fn add_to_queue(&self, target: &str, uri: &str) -> Result<(u16, String)> {
    self
      .player_command(
        target,
        json!({"endpoint": "add_to_queue", "track": {"uri": uri, "provider": "queue"}}),
      )
      .await
  }

  pub async fn transfer(&self, target: &str) -> Result<(u16, String)> {
    let url = format!(
      "{SPCLIENT}/connect-state/v1/connect/transfer/from/{}/to/{}",
      self.device_id, target
    );
    let mut headers = self.http.headers(true).await?;
    headers.insert(
      "X-Spotify-Connection-Id",
      HeaderValue::from_str(&self.connection_id).map_err(Error::other)?,
    );
    let body = json!({
        "options": {"restore_paused": "restore", "restore_position": "extrapolate",
                    "restore_track": "only_current", "license": "premium"},
        "transfer_intent_id": random_hex(16),
        "command_id": random_hex(16),
        "interaction_id": random_hex(16),
    });
    let resp = self
      .http
      .send(HttpMethod::Post, url, headers, serde_json::to_vec(&body)?, 0)
      .await?;
    if !resp.ok() {
      return Err(Error::status("transfer", resp.status, resp.text()));
    }
    Ok((resp.status, resp.text()))
  }

  pub async fn set_volume(&self, target: &str, percent: f64) -> Result<(u16, i32)> {
    let raw = ((percent / 100.0 * 65535.0).round() as i32).clamp(0, 65535);
    let mut cmd = SetVolumeCommand::new();
    cmd.volume = raw;
    let mut lp = ConnectLoggingParams::new();
    lp.interaction_ids.push(random_hex(16));
    cmd.logging_params = MessageField::some(lp);
    cmd.connection_type = "wlan".to_string();
    let mut headers = self.http.headers(false).await?;
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/protobuf"));
    headers.insert(
      "X-Spotify-Connection-Id",
      HeaderValue::from_str(&self.connection_id).map_err(Error::other)?,
    );
    let url = format!(
      "{SPCLIENT}/connect-state/v1/connect/volume/from/{}/to/{}",
      self.device_id, target
    );
    let resp = self
      .http
      .send(HttpMethod::Put, url, headers, cmd.write_to_bytes()?, 0)
      .await?;
    if !resp.ok() {
      return Err(Error::status("set_volume", resp.status, resp.text()));
    }
    Ok((resp.status, raw))
  }
}

fn decode_payload(p: &str, gzipped: bool) -> Result<Vec<u8>> {
  let bytes = base64::engine::general_purpose::STANDARD
    .decode(p)
    .map_err(Error::other)?;
  if !gzipped {
    return Ok(bytes);
  }
  let mut out = Vec::new();
  flate2::read::GzDecoder::new(&bytes[..])
    .read_to_end(&mut out)
    .map_err(Error::other)?;
  Ok(out)
}

pub fn active_device(cluster: &Cluster, me: &str, last_active: Option<&str>) -> Option<String> {
  if !cluster.active_device_id.is_empty() {
    return Some(cluster.active_device_id.clone());
  }
  if let Some(la) = last_active
    && la != me
    && cluster.device.contains_key(la)
  {
    return Some(la.to_string());
  }
  let mut speaker = None;
  let mut any = None;
  for (id, info) in &cluster.device {
    if id == me {
      continue;
    }
    if info.device_type.enum_value_or_default() == DeviceType::SPEAKER && speaker.is_none() {
      speaker = Some(id.clone());
    }
    if any.is_none() {
      any = Some(id.clone());
    }
  }
  speaker.or(any)
}
