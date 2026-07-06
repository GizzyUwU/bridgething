# bridgething Justfile

# --- Path config ---

cross_target := 'aarch64-unknown-linux-gnu'
cross_target_dir := justfile_directory() / 'target-cross'
device_host := env_var_or_default('SUPERBIRD_HOST', 'bridgething.local')
device_bt_mac := env_var_or_default('SUPERBIRD_BT_MAC', '30:E3:D6:03:96:1E')
ssh_args := '-o UserKnownHostsFile=/dev/null -o StrictHostKeyChecking=no -o LogLevel=ERROR'

# --- Local dev ---

run:
  cargo run -p bridgething

build:
  just typescript
  cargo build
  bun run build

fmt:
  cargo +nightly fmt

gateway:
  bun run build -- --filter=@bridgething/gateway
  bun run gateway:example:dev

# --- Codegen ---

typescript:
  cargo run -q -p bridgething-codegen -- ts

swift:
  cargo run -q -p bridgething-codegen -- swift

kotlin:
  cargo run -q -p bridgething-codegen -- kotlin

rust:
  cargo run -q -p bridgething-codegen -- rust

codegen:
  cargo run -q -p bridgething-codegen -- all

spotify-codegen:
  swift build --package-path tools/spotify-codegen
  bash tools/spotify-codegen/scripts/generate-kotlin.sh

# --- Spotify client (uniffi) mobile packaging ---

# Build the spotify rust client as an ios xcframework + swift wrapper.
spotify-ios:
  bash crates/spotify/scripts/build-xcframework.sh

# Build the spotify rust client as android jniLibs + kotlin bindings.
spotify-android:
  bash crates/spotify/scripts/build-jnilibs.sh

# --- Mobile app artifacts ---

# Build a release apk (debug-signed for sideload). Runs on mac and linux.
apk: spotify-android
  bash mobile/scripts/build-apk.sh

# Build an unsigned ipa for sideloading. Requires macos.
ipa: spotify-ios
  bash mobile/scripts/build-ipa.sh

# Build a signed app-store ipa for TestFlight. Requires macos + a configured asc profile.
testflight: spotify-ios
  bash mobile/scripts/build-testflight.sh

goldens:
  UPDATE_GOLDEN=1 cargo test -p libbridgething --test golden golden_vectors_match_fixture_file

# --- Test harness ---

# Host tiers (T1 in-process + T2 chromium): no hardware, runs in parallel.
test-host:
  cargo test -p bridgething-test-harness

# Over-air tier (T3): needs a booted Car Thing with the test-tap daemon + a host
# BT radio. Serial (one radio, one iAP2 link), plus the no-radio bridge proof.
test-device:
  SUPERBIRD_BT_MAC={{device_bt_mac}} cargo test -p bridgething-test-harness --test seam --test t3_infra -- --ignored --test-threads=1 --nocapture

# --- Device iteration ---

# Build the daemon build image. runs at the host's native arch and
# cross-compiles to aarch64, so it is emulation-free on both x86_64 and arm64.
build-image:
  docker build -t bridgething-build -f scripts/cross-aarch64.Dockerfile .

# Cross-build the daemon for the Car Thing.
cross-build: build-image
  docker run --rm -v {{justfile_directory()}}:/work -w /work -v bridgething-cargo-registry:/usr/local/cargo/registry -e CARGO_TARGET_DIR=/work/target-cross bridgething-build cargo build --release -p bridgething --target {{cross_target}} --no-default-features --features superbird --config profile.release.lto=false --config profile.release.codegen-units=32

cross-build-test: build-image
  docker run --rm -v {{justfile_directory()}}:/work -w /work -v bridgething-cargo-registry:/usr/local/cargo/registry -e CARGO_TARGET_DIR=/work/target-cross bridgething-build cargo build --release -p bridgething --target {{cross_target}} --no-default-features --features "superbird,test-tap" --config profile.release.lto=false --config profile.release.codegen-units=32

# Cross-build then push the daemon to /opt/bridgething/daemon/
push: cross-build
  scripts/bridgething-push-daemon {{cross_target_dir}}/{{cross_target}}/release/bridgething

# Cross-build the test-tap binary then push to /opt/bridgething/daemon/
push-test: cross-build-test
  scripts/bridgething-push-daemon {{cross_target_dir}}/{{cross_target}}/release/bridgething

# Push a webapp bundle into /var/bridgething/webapps/<name>/
push-webapp local name="":
  scripts/bridgething-push-webapp {{local}} {{name}}

# SSH into the device. Splits args - watch out for quoting
ssh *args:
  ssh {{ssh_args}} root@{{device_host}} {{args}}

# Tail bridgething.service journal.
logs:
  ssh {{ssh_args}} root@{{device_host}} journalctl -fu bridgething.service

# Tunnel chromium's CDP socket from the device's 127.0.0.1:9222 to the host.
cdp port="9222":
  scripts/bridgething-cdp {{port}}

# Build, push, and tail logs in one shot.
iter: push logs

# --- MFi dev proxy (dev-image only) ---

# Stop bridgething + ALS (via systemd Conflicts=) and start the i2c-3 proxy.
mfi-proxy-up:
  ssh {{ssh_args}} root@{{device_host}} 'systemctl start bridgething-mfi-proxy.service'

# Stop the proxy and bring bridgething + ALS back up.
mfi-proxy-down:
  ssh {{ssh_args}} root@{{device_host}} 'systemctl stop bridgething-mfi-proxy.service; systemctl start bridgething-als.service bridgething.service'

# Tail the proxy's journal.
mfi-proxy-logs:
  ssh {{ssh_args}} root@{{device_host}} journalctl -fu bridgething-mfi-proxy.service

# --- Misc ---

tokei:
  tokei -t Rust,TypeScript,TSX,Kotlin,Swift
