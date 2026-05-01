use crate::Priority;

/// Wire frame plus its priority hint. The codec writes the priority
/// to header byte 5 on encode and reads it back on decode. The
/// `Encoder<T>` impls also accept a bare `T`, defaulting to
/// `Priority::Normal` - existing callers stay source-compatible and
/// only callers that want Bulk lift to the wrapped form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrioritizedFrame<T> {
  pub priority: Priority,
  pub msg: T,
}

impl<T> PrioritizedFrame<T> {
  pub fn normal(msg: T) -> Self {
    Self {
      priority: Priority::Normal,
      msg,
    }
  }

  pub fn bulk(msg: T) -> Self {
    Self {
      priority: Priority::Bulk,
      msg,
    }
  }

  pub fn map<U, F: FnOnce(T) -> U>(self, f: F) -> PrioritizedFrame<U> {
    PrioritizedFrame {
      priority: self.priority,
      msg: f(self.msg),
    }
  }
}
