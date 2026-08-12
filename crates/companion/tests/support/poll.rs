use std::time::Duration;

const DEADLINE: Duration = Duration::from_secs(5);
const POLL: Duration = Duration::from_millis(10);

pub async fn eventually(mut holds: impl FnMut() -> bool) -> bool {
  let deadline = tokio::time::Instant::now() + DEADLINE;
  loop {
    if holds() {
      return true;
    }
    if tokio::time::Instant::now() >= deadline {
      return false;
    }
    tokio::time::sleep(POLL).await;
  }
}
