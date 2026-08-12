use bridgething::DaemonConfig;

const USAGE: &str = "usage: bridgething [--dev]

  --dev   host development mode: no bluetooth radio, loopback binds
  -h      print this message
";

#[tokio::main]
async fn main() {
  let mut config = DaemonConfig::real();

  for arg in std::env::args().skip(1) {
    match arg.as_str() {
      "--dev" => config = DaemonConfig::dev(),
      "-h" | "--help" => return print!("{USAGE}"),
      other => {
        eprint!("bridgething: unrecognized argument {other:?}\n\n{USAGE}");
        std::process::exit(2);
      }
    }
  }

  bridgething::run_daemon(config).await;
}
