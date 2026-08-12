use std::{net::SocketAddr, sync::Arc, time::Duration};

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
use tokio::{net::TcpListener, task::JoinHandle};
use tokio_util::{bytes::Bytes, sync::CancellationToken};
use uuid::Uuid;

use super::{
  BeginRangeError, RangeProxy, RangeTally,
  layout::{self, EmitStep, MULTIPART_BOUNDARY},
  spool::{self, SpoolReader, SpoolWriter},
};
use crate::{
  bluetooth::BluetoothMan,
  transfer::sinks::{AckPolicy, ForwardStream, TransferEvent, TransferSinks},
};

const INGEST_IDLE_TIMEOUT: Duration = Duration::from_secs(180);
const SPOOL_READ_MAX: usize = 64 * 1024;

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

enum Parsed {
  Fresh(Vec<RangeSpec>),
  Resume(u64),
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
  let parsed = match parse_range_header(range_header) {
    Ok(p) => p,
    Err(reason) => return error_response(StatusCode::RANGE_NOT_SATISFIABLE, reason),
  };

  let request_id = Uuid::now_v7();
  let body_rx = state.sinks.bind_forward(request_id, AckPolicy::OnReceipt);
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

  match parsed {
    Parsed::Fresh(ranges) => {
      tracing::debug!(%asset, range_count = ranges.len(), "handling fresh OTA range request");
      handle_fresh(state, request_id, begin, asset, ranges, body_rx).await
    }
    Parsed::Resume(offset) => {
      tracing::debug!(%asset, offset, "handling OTA range resume");
      handle_resume(state, request_id, begin, asset, offset, body_rx).await
    }
  }
}

async fn handle_fresh(
  state: AxumState,
  request_id: Uuid,
  begin: super::RangeBegin,
  asset: String,
  ranges: Vec<RangeSpec>,
  body_rx: ForwardStream,
) -> Response<Body> {
  let reply = match request_companion(&state, request_id, &begin, &asset, ranges).await {
    Ok(reply) => reply,
    Err(resp) => {
      state.proxy.end_range(request_id).await;
      return resp;
    }
  };
  if reply.parts.is_empty() {
    state.proxy.end_range(request_id).await;
    return error_response(StatusCode::BAD_GATEWAY, "companion returned 0 parts");
  }

  state
    .proxy
    .store_ranges(asset, reply.parts.clone(), reply.total_size)
    .await;

  let bl = layout::build(&reply.parts, reply.total_size);
  let plan = layout::plan_from(&bl, 0).expect("offset 0 is always in range");
  let meta = ResponseMeta {
    is_multipart: bl.is_multipart(),
    single: bl.single,
    resume_offset: 0,
    total: reply.total_size,
  };
  finish_response(state, request_id, plan, meta, reply.body, body_rx).await
}

async fn handle_resume(
  state: AxumState,
  request_id: Uuid,
  begin: super::RangeBegin,
  asset: String,
  offset: u64,
  body_rx: ForwardStream,
) -> Response<Body> {
  let (parts, total) = match state.proxy.load_ranges(asset.clone()).await {
    Some(stored) => stored,
    None => {
      state.sinks.unbind(request_id);
      state.proxy.end_range(request_id).await;
      tracing::warn!(%asset, offset, "resume with no remembered ranges; cannot reconstruct body");
      return error_response(StatusCode::RANGE_NOT_SATISFIABLE, "no ranges to resume");
    }
  };
  let bl = layout::build(&parts, total);
  let plan = match layout::plan_from(&bl, offset) {
    Some(plan) => plan,
    None => {
      state.sinks.unbind(request_id);
      state.proxy.end_range(request_id).await;
      return error_response(StatusCode::RANGE_NOT_SATISFIABLE, "resume offset past end of body");
    }
  };
  let meta = ResponseMeta {
    is_multipart: bl.is_multipart(),
    single: bl.single,
    resume_offset: offset,
    total,
  };

  if plan.companion_ranges.is_empty() {
    state.sinks.unbind(request_id);
    state.proxy.end_range(request_id).await;
    let body = Body::from(layout::assemble(&plan.steps, &[]));
    return build_headers(meta, plan.companion_bytes).body(body).unwrap();
  }

  let reply = match request_companion(&state, request_id, &begin, &asset, plan.companion_ranges.clone()).await {
    Ok(reply) => reply,
    Err(resp) => {
      state.proxy.end_range(request_id).await;
      return resp;
    }
  };
  finish_response(state, request_id, plan, meta, reply.body, body_rx).await
}

struct ResponseMeta {
  is_multipart: bool,
  single: Option<RangePart>,
  resume_offset: u64,
  total: u32,
}

async fn request_companion(
  state: &AxumState,
  request_id: Uuid,
  begin: &super::RangeBegin,
  asset: &str,
  ranges: Vec<RangeSpec>,
) -> Result<OtaAssetRangeReply, Response<Body>> {
  let req = OtaAssetRange {
    update_id: begin.update_id.clone(),
    asset: asset.to_string(),
    ranges,
  };
  match state
    .bluetooth
    .gateway_man
    .request_with_id::<OtaAssetRange>(request_id, begin.peer, req)
    .await
  {
    Ok(reply) => Ok(reply),
    Err(RequestError::Domain(rejected)) => {
      tracing::warn!(update_id = %begin.update_id, reason = %rejected.reason, "companion rejected OtaAssetRange");
      Err(error_response(
        StatusCode::BAD_GATEWAY,
        format!("companion rejected: {}", rejected.reason),
      ))
    }
    Err(err) => {
      tracing::warn!(update_id = %begin.update_id, ?err, "OtaAssetRange wire request failed");
      Err(error_response(StatusCode::BAD_GATEWAY, "wire request failed"))
    }
  }
}

async fn finish_response(
  state: AxumState,
  request_id: Uuid,
  plan: layout::EmitPlan,
  meta: ResponseMeta,
  reply_body: TransferBody,
  body_rx: ForwardStream,
) -> Response<Body> {
  let expected = plan.companion_bytes;
  let tally = state.proxy.tally();

  let body = match reply_body {
    TransferBody::Inline(bytes) => {
      state.sinks.unbind(request_id);
      let proxy = state.proxy.clone();
      tokio::spawn(async move { proxy.end_range(request_id).await });
      if bytes.len() as u64 != expected {
        return error_response(
          StatusCode::BAD_GATEWAY,
          "inline body length does not match requested ranges",
        );
      }
      tally.add_expected(expected);
      tally.add_served(expected);
      Body::from(layout::assemble(&plan.steps, &bytes))
    }
    TransferBody::Stream(transfer) => {
      if transfer.id != request_id {
        let proxy = state.proxy.clone();
        tokio::spawn(async move { proxy.end_range(request_id).await });
        return error_response(StatusCode::BAD_GATEWAY, "stream ref id does not match request id");
      }
      if transfer.total_size as u64 != expected {
        let proxy = state.proxy.clone();
        tokio::spawn(async move { proxy.end_range(request_id).await });
        return error_response(StatusCode::BAD_GATEWAY, "stream length does not match requested ranges");
      }
      let spool_dir = crate::paths::state_dir().join("range-spool");
      let (writer, reader) = match spool::create(&spool_dir, &request_id.to_string()).await {
        Ok(pair) => pair,
        Err(err) => {
          tracing::error!(?err, "range spool create failed");
          let proxy = state.proxy.clone();
          tokio::spawn(async move { proxy.end_range(request_id).await });
          return error_response(StatusCode::INTERNAL_SERVER_ERROR, "range spool create failed");
        }
      };
      tally.add_expected(expected);
      tokio::spawn(ingest_pump(body_rx, writer, expected, request_id, tally));
      Body::from_stream(emit_stream(plan.steps, reader, state.proxy, request_id))
    }
  };

  build_headers(meta, expected).body(body).unwrap()
}

async fn ingest_pump(
  mut rx: ForwardStream,
  mut writer: SpoolWriter,
  expected: u64,
  request_id: Uuid,
  tally: Arc<RangeTally>,
) {
  let mut received: u64 = 0;
  loop {
    let event = match tokio::time::timeout(INGEST_IDLE_TIMEOUT, rx.recv()).await {
      Ok(ev) => Ok(ev),
      Err(_) => Err(()),
    };
    let (offset, bytes) = match event {
      Err(()) => {
        tracing::warn!(%request_id, received, expected, "range ingest idle timeout; failing pull");
        writer.fail("range ingest idle timeout");
        return;
      }
      Ok(None) => {
        writer.fail("companion fragment stream closed mid-range");
        return;
      }
      Ok(Some(TransferEvent::Abandon { reason })) => {
        tracing::warn!(%request_id, received, expected, %reason, "companion abandoned range stream");
        writer.fail(format!("companion abandoned range stream: {reason}"));
        return;
      }
      Ok(Some(TransferEvent::Fragment { offset, bytes })) => (offset, bytes),
    };
    if offset as u64 != received {
      tracing::warn!(%request_id, offset, received, "range fragment offset out of order; failing pull");
      writer.fail("companion fragment offset out of order");
      return;
    }
    if received + bytes.len() as u64 > expected {
      tracing::warn!(%request_id, received, expected, "range fragment overshoots declared stream length");
      writer.fail("companion fragment overshoots declared stream length");
      return;
    }
    if let Err(err) = writer.append(&bytes).await {
      tracing::warn!(%request_id, ?err, "range spool write failed");
      writer.fail(format!("range spool write failed: {err}"));
      return;
    }
    received += bytes.len() as u64;
    tally.add_served(bytes.len() as u64);
    if received == expected {
      writer.finish();
      return;
    }
  }
}

fn build_headers(meta: ResponseMeta, expected: u64) -> axum::http::response::Builder {
  let mut builder = Response::builder().status(StatusCode::PARTIAL_CONTENT);
  if meta.is_multipart {
    builder = builder.header(
      header::CONTENT_TYPE,
      HeaderValue::from_str(&format!("multipart/byteranges; boundary={MULTIPART_BOUNDARY}")).unwrap(),
    );
  } else {
    let p = meta.single.expect("single-range response carries its part");
    let start = p.start + meta.resume_offset as u32;
    let end_inclusive = p.start + p.length - 1;
    builder = builder
      .header(header::CONTENT_TYPE, "application/octet-stream")
      .header(header::CONTENT_LENGTH, expected.to_string())
      .header(
        header::CONTENT_RANGE,
        HeaderValue::from_str(&format!("bytes {start}-{end_inclusive}/{}", meta.total)).unwrap(),
      );
  }
  builder
}

fn emit_stream(
  steps: Vec<EmitStep>,
  reader: SpoolReader,
  proxy: RangeProxy,
  request_id: Uuid,
) -> impl futures::Stream<Item = Result<Bytes, std::io::Error>> + Send + 'static {
  let cleanup = OnDropEnd::new(proxy, request_id);
  async_stream::try_stream! {
    let mut reader = reader;
    for step in steps {
      match step {
        EmitStep::Lit(bytes) => {
          yield bytes;
        }
        EmitStep::Data(len) => {
          let mut remaining = len as u64;
          while remaining > 0 {
            let piece = reader.next(remaining.min(SPOOL_READ_MAX as u64) as usize).await?;
            remaining -= piece.len() as u64;
            yield piece;
          }
        }
      }
    }
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

fn error_response(status: StatusCode, body: impl Into<String>) -> Response<Body> {
  Response::builder()
    .status(status)
    .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
    .body(Body::from(body.into()))
    .unwrap()
}

fn parse_range_header(header_value: &str) -> Result<Parsed, &'static str> {
  let trimmed = header_value.trim();
  let payload = trimmed.strip_prefix("bytes=").ok_or("Range must start with bytes=")?;

  if let Some((start, end)) = payload.trim().split_once('-')
    && end.trim().is_empty()
    && !payload.contains(',')
  {
    let start = start.trim();
    if start.is_empty() {
      return Err("suffix ranges 'bytes=-N' are not supported");
    }
    let start: u64 = start.parse().map_err(|_| "resume offset parse failed")?;
    return Ok(Parsed::Resume(start));
  }

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
  if out.is_empty() {
    return Err("Range header parsed to 0 ranges");
  }
  Ok(Parsed::Fresh(out))
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::transfer::sinks::TransferSinks;

  fn spool_temp_dir() -> std::path::PathBuf {
    std::env::temp_dir().join(format!("bridgething-range-serve-test-{}", Uuid::now_v7()))
  }

  #[tokio::test]
  async fn acks_flow_on_receipt_while_reader_stalls() {
    let sinks = TransferSinks::default();
    let request_id = Uuid::now_v7();
    let body_rx = sinks.bind_forward(request_id, AckPolicy::OnReceipt);

    let expected: u64 = 64 * 1024;
    let (writer, mut reader) = spool::create(&spool_temp_dir(), "ack-test").await.unwrap();
    let tally = Arc::new(RangeTally::default());
    let pump = tokio::spawn(ingest_pump(body_rx, writer, expected, request_id, tally.clone()));

    let body: Vec<u8> = (0..expected).map(|i| (i % 251) as u8).collect();
    let mut acks = Vec::new();
    for (i, chunk) in body.chunks(4096).enumerate() {
      if let Some(received) = sinks.fragment(request_id, (i * 4096) as u32, Bytes::copy_from_slice(chunk)) {
        acks.push(received);
      }
      tokio::task::yield_now().await;
    }
    tokio::time::timeout(Duration::from_secs(5), pump)
      .await
      .expect("pump completes without any reader draining the spool")
      .unwrap();

    assert!(!acks.is_empty(), "receipt acks must flow while the reader stalls");
    assert_eq!(acks.last().copied(), Some(expected as u32));
    assert_eq!(tally.snapshot().0, expected);

    let mut drained = Vec::new();
    while (drained.len() as u64) < expected {
      let piece = reader.next(SPOOL_READ_MAX).await.unwrap();
      drained.extend_from_slice(&piece);
    }
    assert_eq!(drained, body);
  }

  #[tokio::test]
  async fn emit_stream_interleaves_headers_with_spooled_data() {
    let parts = vec![
      RangePart { start: 100, length: 50 },
      RangePart {
        start: 1000,
        length: 300,
      },
    ];
    let bl = layout::build(&parts, 100_000);
    let plan = layout::plan_from(&bl, 0).unwrap();
    let companion: Vec<u8> = (0..plan.companion_bytes).map(|i| (i % 251) as u8).collect();
    let want = layout::assemble(&plan.steps, &companion);

    let (mut writer, reader) = spool::create(&spool_temp_dir(), "emit-test").await.unwrap();
    let feeder = {
      let companion = companion.clone();
      tokio::spawn(async move {
        for chunk in companion.chunks(64) {
          writer.append(chunk).await.unwrap();
          tokio::task::yield_now().await;
        }
        writer.finish();
      })
    };

    let stream = emit_stream(plan.steps, reader, super::super::noop_proxy(), Uuid::now_v7());
    let collected: Vec<_> = futures::StreamExt::collect::<Vec<_>>(stream).await;
    let mut got = Vec::new();
    for piece in collected {
      got.extend_from_slice(&piece.unwrap());
    }
    assert_eq!(got, want.to_vec());
    feeder.await.unwrap();
  }

  #[tokio::test]
  async fn abandon_mid_stream_fails_the_body() {
    let sinks = TransferSinks::default();
    let request_id = Uuid::now_v7();
    let body_rx = sinks.bind_forward(request_id, AckPolicy::OnReceipt);

    let (writer, reader) = spool::create(&spool_temp_dir(), "abandon-test").await.unwrap();
    tokio::spawn(ingest_pump(
      body_rx,
      writer,
      1024,
      request_id,
      Arc::new(RangeTally::default()),
    ));

    sinks.fragment(request_id, 0, Bytes::from_static(&[0u8; 512]));
    sinks.abandon(request_id, "link died".into());

    let parts = vec![RangePart { start: 0, length: 1024 }];
    let bl = layout::build(&parts, 4096);
    let plan = layout::plan_from(&bl, 0).unwrap();
    let stream = emit_stream(plan.steps, reader, super::super::noop_proxy(), request_id);
    let collected: Vec<_> = futures::StreamExt::collect::<Vec<_>>(stream).await;
    assert!(
      collected.iter().any(|r| r.is_err()),
      "an abandoned pull must error the HTTP body, not hang"
    );
  }

  fn fresh(header: &str) -> Vec<RangeSpec> {
    match parse_range_header(header).unwrap() {
      Parsed::Fresh(r) => r,
      Parsed::Resume(_) => panic!("expected fresh"),
    }
  }

  #[test]
  fn parses_single_range() {
    assert_eq!(fresh("bytes=0-99"), vec![RangeSpec { start: 0, length: 100 }]);
  }

  #[test]
  fn parses_multi_range() {
    assert_eq!(
      fresh("bytes=0-99,200-299"),
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
  fn open_ended_range_parses_as_resume() {
    assert!(matches!(
      parse_range_header("bytes=134321-"),
      Ok(Parsed::Resume(134321))
    ));
  }

  #[test]
  fn rejects_suffix_range() {
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
