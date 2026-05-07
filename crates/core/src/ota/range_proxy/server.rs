//! axum loopback server. Routes `GET /<update_id>/<asset>` with a
//! `Range:` header into wire `OtaAssetRange` requests, streams the
//! resulting bytes back as `206 Partial Content` (single-range or
//! `multipart/byteranges`). Bound to 127.0.0.1 only.

use std::net::SocketAddr;

use axum::{
  Router,
  body::Body,
  extract::{Path, State},
  http::{HeaderMap, HeaderValue, Response, StatusCode, header},
  routing::get,
};
use libbridgething::{
  RangePart, RangeSpec,
  gateway::{OtaAssetRange, OtaAssetRangeChunk, OtaAssetRangeReply},
  wire::RequestError,
};
use tokio::{net::TcpListener, sync::mpsc, task::JoinHandle};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::{BeginRangeError, MAX_RANGES_PER_REQUEST, RangeProxy};
use crate::bluetooth::BluetoothMan;

/// Multipart/byteranges separator. RFC 7233 §4.1 requires a unique boundary string
const MULTIPART_BOUNDARY: &str = "bridgething-ota-range-boundary";

#[derive(Clone)]
struct AxumState {
  proxy: RangeProxy,
  bluetooth: BluetoothMan,
}

pub(super) async fn spawn(
  proxy: RangeProxy,
  bluetooth: BluetoothMan,
  port: u16,
  cancel: CancellationToken,
) -> std::io::Result<JoinHandle<()>> {
  let listener = TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], port))).await?;
  tracing::info!("ota range proxy listening on 127.0.0.1:{port}");

  let app = Router::new()
    .route("/{asset}", get(handle_range))
    .with_state(AxumState { proxy, bluetooth });

  let handle = tokio::spawn(async move {
    tokio::select! {
      res = axum::serve(listener, app) => {
        if let Err(err) = res {
          tracing::error!("FATAL: ota range proxy server stopped: {err:?}");
        } else {
          tracing::warn!("ota range proxy server exited cleanly");
        }
      }
      _ = cancel.cancelled() => {
        tracing::debug!("ota range proxy server shutting down");
      }
    }
  });
  Ok(handle)
}

async fn handle_range(State(state): State<AxumState>, Path(asset): Path<String>, headers: HeaderMap) -> Response<Body> {
  let range_header = match headers.get(header::RANGE).and_then(|v| v.to_str().ok()) {
    Some(v) => v,
    None => {
      return error_response(
        StatusCode::RANGE_NOT_SATISFIABLE,
        "Range header is required for OTA delta fetch",
      );
    }
  };
  let ranges = match parse_range_header(range_header) {
    Ok(r) => r,
    Err(reason) => return error_response(StatusCode::RANGE_NOT_SATISFIABLE, reason),
  };
  if ranges.is_empty() || ranges.len() > MAX_RANGES_PER_REQUEST {
    return error_response(
      StatusCode::RANGE_NOT_SATISFIABLE,
      "0 or too many ranges (max 10 per request)",
    );
  }

  tracing::debug!(%asset, range_count = ranges.len(), "handling OTA range request");

  let request_id = Uuid::now_v7();
  let (chunk_tx, chunk_rx) = mpsc::channel::<OtaAssetRangeChunk>(super::CHUNK_QUEUE);
  let begin = match state.proxy.begin_range_active(request_id, chunk_tx).await {
    Ok(begin) => begin,
    Err(BeginRangeError::NoActiveOta) => {
      return error_response(StatusCode::CONFLICT, "no OTA in flight");
    }
    Err(BeginRangeError::ProxyDown) => {
      return error_response(StatusCode::INTERNAL_SERVER_ERROR, "range proxy unavailable");
    }
  };

  let req = OtaAssetRange {
    update_id: begin.update_id.clone(),
    asset: asset.clone(),
    ranges,
  };
  let reply = match state
    .bluetooth
    .gateway_man
    .request_with_id::<OtaAssetRange>(request_id, begin.peer, req)
    .await
  {
    Ok(reply) => reply,
    Err(RequestError::Domain(rejected)) => {
      state.proxy.end_range(request_id).await;
      tracing::warn!(update_id = %begin.update_id, reason = %rejected.reason, "companion rejected OtaAssetRange");
      return error_response(
        StatusCode::BAD_GATEWAY,
        format!("companion rejected: {}", rejected.reason),
      );
    }
    Err(err) => {
      state.proxy.end_range(request_id).await;
      tracing::warn!(update_id = %begin.update_id, ?err, "OtaAssetRange wire request failed");
      return error_response(StatusCode::BAD_GATEWAY, "wire request failed");
    }
  };

  build_response(state.proxy, request_id, reply, chunk_rx)
}

fn build_response(
  proxy: RangeProxy,
  request_id: Uuid,
  reply: OtaAssetRangeReply,
  chunk_rx: mpsc::Receiver<OtaAssetRangeChunk>,
) -> Response<Body> {
  let total = reply.total_size;
  let parts = reply.parts;
  if parts.is_empty() {
    let proxy = proxy.clone();
    tokio::spawn(async move { proxy.end_range(request_id).await });
    return error_response(StatusCode::BAD_GATEWAY, "companion returned 0 parts");
  }

  if parts.len() == 1 {
    let p = parts[0];
    let (start, end_inclusive) = (p.start, p.start + p.length - 1);
    let stream = body_stream_single_part(p, chunk_rx, proxy, request_id);
    let body = Body::from_stream(stream);
    Response::builder()
      .status(StatusCode::PARTIAL_CONTENT)
      .header(header::CONTENT_TYPE, "application/octet-stream")
      .header(header::CONTENT_LENGTH, p.length.to_string())
      .header(
        header::CONTENT_RANGE,
        HeaderValue::from_str(&format!("bytes {start}-{end_inclusive}/{total}")).unwrap(),
      )
      .body(body)
      .unwrap()
  } else {
    let stream = body_stream_multipart(parts.clone(), total, chunk_rx, proxy, request_id);
    let body = Body::from_stream(stream);
    Response::builder()
      .status(StatusCode::PARTIAL_CONTENT)
      .header(
        header::CONTENT_TYPE,
        HeaderValue::from_str(&format!("multipart/byteranges; boundary={MULTIPART_BOUNDARY}")).unwrap(),
      )
      .body(body)
      .unwrap()
  }
}

fn parse_range_header(header_value: &str) -> Result<Vec<RangeSpec>, &'static str> {
  let trimmed = header_value.trim();
  let payload = trimmed.strip_prefix("bytes=").ok_or("Range must start with bytes=")?;
  let mut out = Vec::new();
  for piece in payload.split(',') {
    let piece = piece.trim();
    let (start, end) = piece.split_once('-').ok_or("range piece missing '-'")?;
    let start = start.trim();
    let end = end.trim();
    if start.is_empty() || end.is_empty() {
      return Err("only fully-bounded ranges 'a-b' are supported");
    }
    let start: u32 = start.parse().map_err(|_| "range start parse failed")?;
    let end: u32 = end.parse().map_err(|_| "range end parse failed")?;
    if end < start {
      return Err("range end < start");
    }
    let length = end
      .checked_sub(start)
      .and_then(|d| d.checked_add(1))
      .ok_or("range length overflow")?;
    out.push(RangeSpec { start, length });
  }
  Ok(out)
}

fn body_stream_single_part(
  part: RangePart,
  chunk_rx: mpsc::Receiver<OtaAssetRangeChunk>,
  proxy: RangeProxy,
  request_id: Uuid,
) -> impl futures::Stream<Item = Result<bytes::Bytes, std::io::Error>> + Send + 'static {
  let total = part.length as u64;
  let cleanup = OnDropEnd::new(proxy, request_id);
  async_stream::try_stream! {
    let mut produced: u64 = 0;
    let mut rx = chunk_rx;
    while produced < total {
      let chunk = match rx.recv().await {
        Some(c) => c,
        None => {
          Err(io_err("companion chunk channel closed mid-stream"))?;
          unreachable!();
        }
      };
      let bytes = bytes::Bytes::from(chunk.bytes);
      produced += bytes.len() as u64;
      if produced > total {
        Err(io_err("companion sent more bytes than the part declared"))?;
        unreachable!();
      }
      yield bytes;
      if chunk.last {
        break;
      }
    }
    if produced != total {
      Err(io_err("companion stream ended before declared length"))?;
    }
    drop(cleanup);
  }
}

fn body_stream_multipart(
  parts: Vec<RangePart>,
  total_size: u32,
  chunk_rx: mpsc::Receiver<OtaAssetRangeChunk>,
  proxy: RangeProxy,
  request_id: Uuid,
) -> impl futures::Stream<Item = Result<bytes::Bytes, std::io::Error>> + Send + 'static {
  let cleanup = OnDropEnd::new(proxy, request_id);
  async_stream::try_stream! {
    let mut rx = chunk_rx;
    for (idx, part) in parts.iter().enumerate() {
      let header = format!(
        "\r\n--{boundary}\r\nContent-Type: application/octet-stream\r\nContent-Range: bytes {start}-{end}/{total}\r\n\r\n",
        boundary = MULTIPART_BOUNDARY,
        start = part.start,
        end = part.start + part.length - 1,
        total = total_size,
      );
      yield bytes::Bytes::from(header);

      let part_total = part.length as u64;
      let mut produced: u64 = 0;
      while produced < part_total {
        let chunk = match rx.recv().await {
          Some(c) => c,
          None => {
            Err(io_err("companion chunk channel closed mid-stream"))?;
            unreachable!();
          }
        };
        if chunk.part_index as usize != idx {
          Err(io_err("companion chunk part_index out of order"))?;
          unreachable!();
        }
        let bytes = bytes::Bytes::from(chunk.bytes);
        produced += bytes.len() as u64;
        if produced > part_total {
          Err(io_err("companion sent more bytes than the part declared"))?;
          unreachable!();
        }
        yield bytes;
        if chunk.last && idx + 1 < parts.len() {
          Err(io_err("companion set last:true mid-multipart"))?;
          unreachable!();
        }
      }
      if produced != part_total {
        Err(io_err("companion stream ended before declared part length"))?;
      }
    }
    yield bytes::Bytes::from(format!("\r\n--{MULTIPART_BOUNDARY}--\r\n"));
    drop(cleanup);
  }
}

struct OnDropEnd {
  proxy: RangeProxy,
  request_id: Uuid,
}

impl OnDropEnd {
  fn new(proxy: RangeProxy, request_id: Uuid) -> Self {
    Self { proxy, request_id }
  }
}

impl Drop for OnDropEnd {
  fn drop(&mut self) {
    let proxy = self.proxy.clone();
    let request_id = self.request_id;
    tokio::spawn(async move { proxy.end_range(request_id).await });
  }
}

fn io_err(msg: &'static str) -> std::io::Error {
  std::io::Error::other(msg)
}

fn error_response(status: StatusCode, body: impl Into<String>) -> Response<Body> {
  Response::builder()
    .status(status)
    .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
    .body(Body::from(body.into()))
    .unwrap()
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn parses_single_range() {
    let r = parse_range_header("bytes=0-99").unwrap();
    assert_eq!(r, vec![RangeSpec { start: 0, length: 100 }]);
  }

  #[test]
  fn parses_multi_range() {
    let r = parse_range_header("bytes=0-99,200-299").unwrap();
    assert_eq!(
      r,
      vec![
        RangeSpec { start: 0, length: 100 },
        RangeSpec {
          start: 200,
          length: 100
        },
      ]
    );
  }

  #[test]
  fn rejects_open_ended_ranges() {
    assert!(parse_range_header("bytes=0-").is_err());
    assert!(parse_range_header("bytes=-100").is_err());
  }

  #[test]
  fn rejects_inverted_range() {
    assert!(parse_range_header("bytes=10-5").is_err());
  }

  #[test]
  fn rejects_missing_prefix() {
    assert!(parse_range_header("0-99").is_err());
  }
}
