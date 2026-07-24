use std::{env, path::PathBuf};

fn main() {
  let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
  let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));

  let include_dir = manifest_dir.join("vendor/swupdate/include");
  let network_ipc = include_dir.join("network_ipc.h");
  let progress_ipc = include_dir.join("progress_ipc.h");

  for path in [&network_ipc, &progress_ipc] {
    println!("cargo:rerun-if-changed={}", path.display());
    assert!(
      path.exists(),
      "missing header: {} (run `git submodule update --init`)",
      path.display()
    );
  }

  let target = env::var("TARGET").expect("TARGET set by cargo");

  let bindings = bindgen::Builder::default()
    .header(network_ipc.to_string_lossy().into_owned())
    .header(progress_ipc.to_string_lossy().into_owned())
    .clang_arg(format!("-I{}", include_dir.display()))
    .clang_arg(format!("--target={target}"))
    .allowlist_function("swupdate_.*")
    .allowlist_function("ipc_.*")
    .allowlist_function("progress_ipc_.*")
    .allowlist_function("get_(ctrl|prog)_socket")
    .allowlist_type("swupdate_request")
    .allowlist_type("ipc_message")
    .allowlist_type("msgdata")
    .allowlist_type("msgtype")
    .allowlist_type("sourcetype")
    .allowlist_type("RECOVERY_STATUS")
    .allowlist_type("run_type")
    .allowlist_type("progress_msg")
    .allowlist_type("progress_cause.*")
    .allowlist_type("progress_connect_ack")
    .allowlist_var("IPC_MAGIC")
    .allowlist_var("SWUPDATE_API_VERSION")
    .allowlist_var("SOCKET_PROGRESS_PATH")
    .allowlist_var("PROGRESS_API_.*")
    .allowlist_var("PROGRESS_CONNECT_ACK_MAGIC")
    .allowlist_var("PRINFOSIZE")
    .derive_default(true)
    .derive_debug(true)
    .generate_comments(false)
    .layout_tests(false)
    .generate()
    .expect("bindgen failed to generate libswupdate bindings");

  let out_path = out_dir.join("bindings.rs");
  bindings.write_to_file(&out_path).expect("failed to write bindings.rs");

  println!("cargo:rustc-link-lib=swupdate");
  println!("cargo:include={}", include_dir.display());
}
