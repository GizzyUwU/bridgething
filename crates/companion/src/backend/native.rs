use std::sync::Arc;

use bridgething_io as io;
use uuid::Uuid;

use crate::backend::net::{
  HttpDownloadSink, HttpRequest, HttpSink, HttpTransport, WsConnect, WsFrame, WsInbox, WsTransport,
};

#[derive(Default)]
pub struct NativeHttp(io::ReqwestTransport);

impl NativeHttp {
  pub fn new(config: io::ReqwestConfig) -> Self {
    Self(io::ReqwestTransport::new(config))
  }
}

impl HttpTransport for NativeHttp {
  fn execute(&self, request: HttpRequest, sink: Arc<HttpSink>) {
    io::HttpTransport::execute(&self.0, request.into(), sink.inner().clone());
  }

  fn download(&self, request: HttpRequest, sink: Arc<HttpDownloadSink>) {
    io::HttpTransport::download(&self.0, request.into(), sink.inner().clone());
  }
}

#[derive(Default)]
pub struct NativeWs(io::TungsteniteTransport);

impl NativeWs {
  pub fn new() -> Self {
    Self::default()
  }
}

impl WsTransport for NativeWs {
  fn connect(&self, connect: WsConnect, inbox: Arc<WsInbox>) {
    let Some(id) = connection(&connect.id) else { return };
    io::WsTransport::connect(
      &self.0,
      io::WsConnect {
        id,
        url: connect.url,
        protocols: connect.protocols,
        headers: connect.headers.into_iter().map(Into::into).collect(),
      },
      inbox.inner().clone(),
    );
  }

  fn send(&self, id: String, frame: WsFrame) {
    if let Some(id) = connection(&id) {
      io::WsTransport::send(&self.0, id, frame.into());
    }
  }

  fn disconnect(&self, id: String, code: Option<u16>, reason: Option<String>) {
    if let Some(id) = connection(&id) {
      io::WsTransport::disconnect(&self.0, id, code, reason);
    }
  }
}

fn connection(id: &str) -> Option<Uuid> {
  match Uuid::parse_str(id) {
    Ok(id) => Some(id),
    Err(_) => {
      tracing::warn!(%id, "a native socket was asked for under an unparseable connection id; dropping");
      None
    }
  }
}
