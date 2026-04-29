pub fn init_logger(level_directive: Option<String>) {
  use tracing::metadata::LevelFilter;
  use tracing_subscriber::fmt::format::FmtSpan;
  use tracing_subscriber::{
    EnvFilter, Layer, filter::Directive, fmt, prelude::__tracing_subscriber_SubscriberExt, util::SubscriberInitExt,
  };

  let default_directive = Directive::from(LevelFilter::INFO);
  let filter_directives = if let Some(directive) = level_directive {
    directive
  } else if let Ok(filter) = std::env::var("RUST_LOG") {
    filter
  } else {
    "bridgething_adapter=info,libbridgething=info".to_string()
  };

  let filter = EnvFilter::builder()
    .with_default_directive(default_directive)
    .parse_lossy(filter_directives);

  tracing_subscriber::registry()
    .with(fmt::layer().with_span_events(FmtSpan::CLOSE).with_filter(filter))
    .init();

  tracing::info!("initialized logger");
}
