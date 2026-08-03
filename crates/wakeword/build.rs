use std::path::Path;

fn main() {
  let blob = Path::new(env!("CARGO_MANIFEST_DIR")).join("models/embedding_stream.btww");
  println!("cargo:rerun-if-changed={}", blob.display());
  if !blob.exists() {
    panic!(
      "missing {}: run scripts/bridgething-fetch-wakeword-models to download the pinned graphs and regenerate it",
      blob.display()
    );
  }
}
