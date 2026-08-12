use bridgething_companion::api::SessionEvent;

use crate::backends::Heard;

impl Heard {
  pub fn events(&self) -> Vec<SessionEvent> {
    self.0.lock().unwrap().clone()
  }
}
