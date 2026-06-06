//! axum loopback server. Routes `GET /<update_id>/<asset>` with a
//! `Range:` header into wire `OtaAssetRange` requests, streams the
//! resulting bytes back as `206 Partial Content` (single-range or
//! `multipart/byteranges`). Bound to 127.0.0.1 only.
//!
//! The reply's `TransferBody` is either inline (small ranges, assembled
//! directly) or a fragment stream with stream-relative offsets over the
//! concatenated parts; fragments need not align to part boundaries, so
//! the multipart writer splits them as it goes.

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
  gateway::{OtaAssetRange, OtaAssetRangeReply, TransferBody},
  wire::RequestError,
};
use tokio::{net::TcpListener, sync::mpsc, task::JoinHandle};
use tokio_util::{
  bytes::{Bytes, BytesMut},
  sync::CancellationToken,
};
use uuid::Uuid;

use super::{BeginRangeError, RangeProxy};
use crate::{
  bluetooth::BluetoothMan,
  transfer::sinks::{TransferEvent, TransferSinks},
};

/// Multipart/byteranges separator. RFC 7233 §4.1 requires a unique boundary string
const MULTIPART_BOUNDARY: &str = "bridgething-ota-range-boundary";

#[derive(Clone)]
struct AxumState {
  proxy: RangeProxy,
  bluetooth: BluetoothMan,
  sinks: TransferSinks,
}

pub(super) async fn spawn(
  proxy: RangeProxy,
  bluetooth: BluetoothMan,
  sinks: TransferSinks,
  port: u16,
  cancel: CancellationToken,
) -> std::io::Result<JoinHandle<()>> {
  let listener = TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], port))).await?;
  tracing::info!("ota range proxy listening on 127.0.0.1:{port}");

  let app = Router::new()
    .route("/{asset}", get(handle_range))
    .with_state(AxumState {
      proxy,
      bluetooth,
      sinks,
    });

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
  if ranges.is_empty() {
    return error_response(StatusCode::RANGE_NOT_SATISFIABLE, "Range header parsed to 0 ranges");
  }

  tracing::debug!(%asset, range_count = ranges.len(), "handling OTA range request");

  let request_id = Uuid::now_v7();
  // bind before the wire request so fragments racing the reply are kept
  let body_rx = state.sinks.bind_forward(request_id);
  let begin = match state.proxy.begin_range_active(request_id).await {
    Ok(begin) => begin,
    Err(BeginRangeError::NoActiveOta) => {
      state.sinks.unbind(request_id);
      return error_response(StatusCode::CONFLICT, "no OTA in flight");
    }
    Err(BeginRangeError::ProxyDown) => {
      state.sinks.unbind(request_id);
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

  build_response(state.proxy, request_id, reply, body_rx)
}

fn build_response(
  proxy: RangeProxy,
  request_id: Uuid,
  reply: OtaAssetRangeReply,
  body_rx: mpsc::Receiver<TransferEvent>,
) -> Response<Body> {
  let total = reply.total_size;
  let parts = reply.parts;
  let end_now = |proxy: RangeProxy| {
    tokio::spawn(async move { proxy.end_range(request_id).await });
  };
  if parts.is_empty() {
    end_now(proxy);
    return error_response(StatusCode::BAD_GATEWAY, "companion returned 0 parts");
  }
  let expected: u64 = parts.iter().map(|p| p.length as u64).sum();

  let body = match reply.body {
    TransferBody::Inline(bytes) => {
      end_now(proxy);
      if bytes.len() as u64 != expected {
        return error_response(
          StatusCode::BAD_GATEWAY,
          "inline body length does not match declared parts",
        );
      }
      if parts.len() == 1 {
        Body::from(bytes)
      } else {
        Body::from(assemble_multipart_inline(&parts, total, &bytes))
      }
    }
    TransferBody::Stream(transfer) => {
      if transfer.id != request_id {
        end_now(proxy);
        return error_response(StatusCode::BAD_GATEWAY, "stream ref id does not match request id");
      }
      if transfer.total_size as u64 != expected {
        end_now(proxy);
        return error_response(StatusCode::BAD_GATEWAY, "stream length does not match declared parts");
      }
      if parts.len() == 1 {
        Body::from_stream(body_stream_single_part(expected, body_rx, proxy, request_id))
      } else {
        Body::from_stream(body_stream_multipart(parts.clone(), total, body_rx, proxy, request_id))
      }
    }
  };

  if parts.len() == 1 {
    let p = parts[0];
    let (start, end_inclusive) = (p.start, p.start + p.length - 1);
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

fn multipart_part_header(part: &RangePart, total: u32) -> String {
  format!(
    "\r\n--{boundary}\r\nContent-Type: application/octet-stream\r\nContent-Range: bytes {start}-{end}/{total}\r\n\r\n",
    boundary = MULTIPART_BOUNDARY,
    start = part.start,
    end = part.start + part.length - 1,
  )
}

fn assemble_multipart_inline(parts: &[RangePart], total: u32, bytes: &[u8]) -> Bytes {
  let mut out = BytesMut::new();
  let mut consumed = 0usize;
  for part in parts {
    out.extend_from_slice(multipart_part_header(part, total).as_bytes());
    let next = consumed + part.length as usize;
    out.extend_from_slice(&bytes[consumed..next]);
    consumed = next;
  }
  out.extend_from_slice(format!("\r\n--{MULTIPART_BOUNDARY}--\r\n").as_bytes());
  out.freeze()
}

async fn next_fragment(rx: &mut mpsc::Receiver<TransferEvent>, produced: u64) -> Result<Bytes, std::io::Error> {
  match rx.recv().await {
    None => Err(io_err("companion fragment stream closed mid-range")),
    Some(TransferEvent::Abandon { reason }) => Err(std::io::Error::other(format!(
      "companion abandoned range stream: {reason}"
    ))),
    Some(TransferEvent::Fragment { offset, bytes }) => {
      if offset as u64 != produced {
        return Err(io_err("companion fragment offset out of order"));
      }
      Ok(bytes)
    }
  }
}

fn body_stream_single_part(
  total: u64,
  body_rx: mpsc::Receiver<TransferEvent>,
  proxy: RangeProxy,
  request_id: Uuid,
) -> impl futures::Stream<Item = Result<Bytes, std::io::Error>> + Send + 'static {
  let cleanup = OnDropEnd::new(proxy, request_id);
  async_stream::try_stream! {
    let mut produced: u64 = 0;
    let mut rx = body_rx;
    while produced < total {
      let bytes = next_fragment(&mut rx, produced).await?;
      if produced + bytes.len() as u64 > total {
        Err(io_err("companion sent more bytes than the range declared"))?;
        unreachable!();
      }
      produced += bytes.len() as u64;
      yield bytes;
    }
    drop(cleanup);
  }
}

fn body_stream_multipart(
  parts: Vec<RangePart>,
  total_size: u32,
  body_rx: mpsc::Receiver<TransferEvent>,
  proxy: RangeProxy,
  request_id: Uuid,
) -> impl futures::Stream<Item = Result<Bytes, std::io::Error>> + Send + 'static {
  let cleanup = OnDropEnd::new(proxy, request_id);
  async_stream::try_stream! {
    let mut rx = body_rx;
    let mut produced: u64 = 0;
    // fragments are stream-relative over the concatenated parts and may span part boundaries
    let mut leftover = Bytes::new();
    for part in parts.iter() {
      yield Bytes::from(multipart_part_header(part, total_size));

      let mut remaining = part.length as u64;
      while remaining > 0 {
        if leftover.is_empty() {
          leftover = next_fragment(&mut rx, produced).await?;
          if produced + leftover.len() as u64 > parts.iter().map(|p| p.length as u64).sum::<u64>() {
            Err(io_err("companion sent more bytes than the ranges declared"))?;
            unreachable!();
          }
        }
        let take = remaining.min(leftover.len() as u64) as usize;
        let piece = leftover.split_to(take);
        produced += take as u64;
        remaining -= take as u64;
        yield piece;
      }
    }
    yield Bytes::from(format!("\r\n--{MULTIPART_BOUNDARY}--\r\n"));
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

  #[test]
  fn inline_multipart_assembles_parts_in_order() {
    let parts = vec![RangePart { start: 0, length: 2 }, RangePart { start: 10, length: 3 }];
    let assembled = assemble_multipart_inline(&parts, 100, b"abcde");
    let text = String::from_utf8_lossy(&assembled);
    assert!(text.contains("bytes 0-1/100"));
    assert!(text.contains("bytes 10-12/100"));
    let ab = text.find("ab").unwrap();
    let cde = text.find("cde").unwrap();
    assert!(ab < cde);
    assert!(text.trim_end().ends_with(&format!("--{MULTIPART_BOUNDARY}--")));
  }
}
