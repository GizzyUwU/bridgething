#![cfg(feature = "native-io")]

use std::{
  sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
  },
  time::Duration,
};

use bridgething_io::{DownloadBody, HttpExecutor, HttpHeader, HttpMethod, HttpRequest, ReqwestTransport};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const HOLD: Duration = Duration::from_millis(100);
const TICK: Duration = Duration::from_millis(5);

async fn body_server(payload: Vec<u8>) -> u16 {
  let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("a free port");
  let port = listener.local_addr().expect("a bound address").port();
  tokio::spawn(async move {
    let (mut socket, _) = listener.accept().await.expect("a client");
    let mut seen = Vec::new();
    let mut buf = [0u8; 1024];
    while !seen.windows(4).any(|w| w == b"\r\n\r\n") {
      let read = socket.read(&mut buf).await.expect("a request");
      if read == 0 {
        return;
      }
      seen.extend_from_slice(&buf[..read]);
    }
    let head = format!(
      "HTTP/1.1 200 OK\r\ncontent-type: application/octet-stream\r\ncontent-length: {}\r\n\r\n",
      payload.len()
    );
    socket.write_all(head.as_bytes()).await.expect("the head goes out");
    socket.write_all(&payload).await.expect("the body goes out");
    socket.flush().await.expect("flushed");
  });
  port
}

struct StallingBody {
  ticks: Arc<AtomicUsize>,
  during: Arc<Mutex<Vec<usize>>>,
}

impl DownloadBody for StallingBody {
  fn on_response(&mut self, status: u16, _headers: &[HttpHeader], _content_length: Option<u64>) -> bool {
    (200..300).contains(&status)
  }

  fn write(&mut self, _chunk: &[u8]) -> Result<(), String> {
    let before = self.ticks.load(Ordering::SeqCst);
    std::thread::sleep(HOLD);
    self
      .during
      .lock()
      .unwrap()
      .push(self.ticks.load(Ordering::SeqCst) - before);
    Ok(())
  }
}

#[tokio::test]
async fn a_blocking_body_write_does_not_stall_the_runtime() {
  let port = body_server(vec![0xA5; 8 * 1024]).await;

  let ticks = Arc::new(AtomicUsize::new(0));
  let ticker = {
    let ticks = ticks.clone();
    tokio::spawn(async move {
      loop {
        tokio::time::sleep(TICK).await;
        ticks.fetch_add(1, Ordering::SeqCst);
      }
    })
  };

  let during = Arc::new(Mutex::new(Vec::new()));
  let exec = HttpExecutor::new(Arc::new(ReqwestTransport::default()));
  let outcome = exec
    .download(
      HttpRequest {
        method: HttpMethod::Get,
        url: format!("http://127.0.0.1:{port}/image.swu"),
        headers: Vec::new(),
        body: Vec::new(),
        timeout_ms: 0,
      },
      Box::new(StallingBody {
        ticks: ticks.clone(),
        during: during.clone(),
      }),
    )
    .await
    .expect("the download completes");
  ticker.abort();

  assert_eq!(outcome.received, 8 * 1024, "every byte reached the writer");
  let during = during.lock().unwrap();
  assert!(!during.is_empty(), "the writer saw at least one chunk");
  assert!(
    during.iter().all(|advanced| *advanced >= 4),
    "the runtime kept scheduling other tasks across every {HOLD:?} body write, ticks advanced: {during:?}"
  );
}
