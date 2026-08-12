use std::{future::Future, pin::Pin};

use libbridgething::{Priority, protocol::Compress};

pub type AfterResponse = Pin<Box<dyn Future<Output = ()> + Send>>;

pub struct Reply<T> {
  pub response: T,
  pub priority: Option<Priority>,
  pub compress: Compress,
  pub after: Option<AfterResponse>,
}

impl<T> Reply<T> {
  pub fn new(response: T) -> Self {
    Self {
      response,
      priority: None,
      compress: Compress::Auto,
      after: None,
    }
  }

  pub fn lane(mut self, priority: Priority) -> Self {
    self.priority = Some(priority);
    self
  }

  pub fn compressed(mut self, compress: Compress) -> Self {
    self.compress = compress;
    self
  }

  pub fn then(mut self, after: impl Future<Output = ()> + Send + 'static) -> Self {
    self.after = Some(Box::pin(after));
    self
  }
}

impl<T: std::fmt::Debug> std::fmt::Debug for Reply<T> {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("Reply")
      .field("response", &self.response)
      .field("priority", &self.priority)
      .field("compress", &self.compress)
      .field("after", &self.after.is_some())
      .finish()
  }
}

impl<T> From<T> for Reply<T> {
  fn from(response: T) -> Self {
    Self::new(response)
  }
}
