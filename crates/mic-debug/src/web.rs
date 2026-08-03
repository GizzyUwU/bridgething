use std::{convert::Infallible, net::SocketAddr, time::Duration};

use axum::{
  Json, Router,
  extract::State,
  http::{StatusCode, header},
  response::{IntoResponse, Sse, sse::Event},
  routing::{get, post},
};
use serde::Deserialize;
use tokio::sync::mpsc;
use tokio_stream::{Stream, StreamExt, wrappers::WatchStream};

use crate::{
  input::{Command, MarkKind},
  status::Shared,
  usb,
};

const PAGE: &str = include_str!("../ui/index.html");

#[derive(Clone)]
struct Ctx {
  shared: Shared,
  commands: mpsc::Sender<Command>,
}

pub const READY_FLAG: &str = "/run/mic-debug.ready";

pub async fn bind(addr: SocketAddr) -> std::io::Result<tokio::net::TcpListener> {
  let _ = tokio::fs::remove_file(READY_FLAG).await;
  let listener = tokio::net::TcpListener::bind(addr).await?;
  tokio::fs::write(READY_FLAG, b"").await?;
  tracing::info!(%addr, "debug ui listening");
  Ok(listener)
}

pub async fn serve(
  listener: tokio::net::TcpListener,
  shared: Shared,
  commands: mpsc::Sender<Command>,
) -> std::io::Result<()> {
  let app = Router::new()
    .route("/", get(page))
    .route("/status", get(status))
    .route("/events", get(events))
    .route("/mark", post(mark))
    .route("/tag", post(tag))
    .route("/start", post(start))
    .route("/stop", post(stop))
    .route("/usb-role", post(usb_role))
    .with_state(Ctx { shared, commands });
  axum::serve(listener, app).await
}

async fn page() -> impl IntoResponse {
  ([(header::CONTENT_TYPE, "text/html; charset=utf-8")], PAGE)
}

async fn status(State(ctx): State<Ctx>) -> impl IntoResponse {
  Json(ctx.shared.snapshot())
}

async fn events(State(ctx): State<Ctx>) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
  let shared = ctx.shared.clone();
  let stream = WatchStream::new(ctx.shared.subscribe())
    .map(move |_| {
      Ok(
        Event::default()
          .json_data(shared.snapshot())
          .expect("status serialises"),
      )
    })
    .throttle(Duration::from_millis(100));
  Sse::new(stream).keep_alive(axum::response::sse::KeepAlive::new().interval(Duration::from_secs(5)))
}

#[derive(Deserialize)]
struct MarkBody {
  kind: MarkKind,
}

async fn mark(State(ctx): State<Ctx>, Json(body): Json<MarkBody>) -> StatusCode {
  send(&ctx, Command::Mark(body.kind)).await
}

async fn tag(State(ctx): State<Ctx>) -> StatusCode {
  send(&ctx, Command::CycleTag).await
}

async fn start(State(ctx): State<Ctx>) -> StatusCode {
  send(&ctx, Command::StartSession).await
}

async fn stop(State(ctx): State<Ctx>) -> StatusCode {
  send(&ctx, Command::StopSession).await
}

#[derive(Deserialize)]
struct RoleBody {
  role: String,
}

async fn usb_role(State(ctx): State<Ctx>, Json(body): Json<RoleBody>) -> StatusCode {
  if body.role != "host" && body.role != "device" {
    return StatusCode::BAD_REQUEST;
  }
  if body.role == "device" {
    let _ = ctx.commands.send(Command::StopSession).await;
  }
  match usb::set_role(&body.role) {
    Ok(()) => {
      let role = usb::role();
      ctx.shared.update(|status| status.usb_role = role);
      StatusCode::NO_CONTENT
    }
    Err(err) => {
      ctx.shared.alert(err);
      StatusCode::INTERNAL_SERVER_ERROR
    }
  }
}

async fn send(ctx: &Ctx, command: Command) -> StatusCode {
  match ctx.commands.send(command).await {
    Ok(()) => StatusCode::NO_CONTENT,
    Err(_) => StatusCode::SERVICE_UNAVAILABLE,
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn the_page_is_self_contained() {
    assert!(
      !PAGE.contains("<script src="),
      "the kiosk has no network to fetch anything from"
    );
    assert!(!PAGE.contains("<link rel=\"stylesheet\""));
  }

  #[test]
  fn mark_kinds_arrive_over_the_wire_in_the_shape_the_page_sends() {
    let body: MarkBody = serde_json::from_str(r#"{"kind":"falseAlarm"}"#).expect("parse");
    assert_eq!(body.kind, MarkKind::FalseAlarm);
  }
}
