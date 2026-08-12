use bridgething_companion::backend::{LogLevel, LogSink};

pub struct Quiet;

impl LogSink for Quiet {
  fn on_line(&self, _level: LogLevel, _target: String, _message: String) {}
}
