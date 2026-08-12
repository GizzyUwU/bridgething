use std::process::Command;

const DENIED: [&str; 7] = ["reqwest", "protobuf", "native-tls", "rustls", "opus", "spotify", "nlu"];

#[test]
fn the_delivery_graph_names_no_platform_or_provider_dependency() {
  let out = Command::new(env!("CARGO"))
    .current_dir(env!("CARGO_MANIFEST_DIR"))
    .args([
      "tree",
      "--package",
      "bridgething-delivery",
      "--edges",
      "normal",
      "--prefix",
      "none",
      "--format",
      "{p}",
    ])
    .output()
    .expect("cargo tree runs");
  assert!(
    out.status.success(),
    "cargo tree failed: {}",
    String::from_utf8_lossy(&out.stderr)
  );

  let tree = String::from_utf8(out.stdout).expect("cargo tree emits utf8");
  let mut hits: Vec<&str> = tree
    .lines()
    .filter_map(|line| line.split_whitespace().next())
    .filter(|name| DENIED.iter().any(|denied| name.contains(denied)))
    .collect();
  hits.sort_unstable();
  hits.dedup();

  assert!(hits.is_empty(), "delivery must not depend on {hits:?}");
}
