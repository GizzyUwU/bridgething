use std::sync::{
  Arc,
  atomic::{AtomicUsize, Ordering},
};

use bridgething_companion::backend::ImageScaler;
use bridgething_io::{HttpDownloadSink, HttpRequest, HttpResponse, HttpSink, HttpTransport};

#[derive(Default)]
pub struct ArtProbe {
  fetches: AtomicUsize,
}

impl ArtProbe {
  pub fn new() -> Arc<Self> {
    Arc::new(Self::default())
  }

  pub fn fetches(&self) -> usize {
    self.fetches.load(Ordering::SeqCst)
  }
}

impl HttpTransport for ArtProbe {
  fn execute(&self, _request: HttpRequest, sink: Arc<HttpSink>) {
    self.fetches.fetch_add(1, Ordering::SeqCst);
    sink.complete(HttpResponse {
      status: 200,
      headers: Vec::new(),
      body: b"master".to_vec(),
    });
  }

  fn download(&self, _request: HttpRequest, sink: Arc<HttpDownloadSink>) {
    sink.on_failed("the art probe has no streaming arm".into());
  }
}

#[derive(Default)]
pub struct TagScaler {
  scales: AtomicUsize,
}

impl TagScaler {
  pub fn new() -> Arc<Self> {
    Arc::new(Self::default())
  }

  pub fn scales(&self) -> usize {
    self.scales.load(Ordering::SeqCst)
  }
}

impl ImageScaler for TagScaler {
  fn downsample_jpeg(&self, bytes: Vec<u8>, max_edge: u32, _quality: f32) -> Option<Vec<u8>> {
    self.scales.fetch_add(1, Ordering::SeqCst);
    Some(format!("{}@{max_edge}", String::from_utf8_lossy(&bytes)).into_bytes())
  }
}
