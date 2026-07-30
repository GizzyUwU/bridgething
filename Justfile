# bridgething Justfile

# --- Path config ---

cross_target := 'aarch64-unknown-linux-gnu'
cross_target_dir := justfile_directory() / 'target-cross'
cross_release_dir := justfile_directory() / 'target-cross-release'
device_features := 'superbird'
dev_profile := '--config profile.release.lto=false --config profile.release.codegen-units=32'
release_build := 'cargo build --release --locked -p bridgething --target ' + cross_target + ' --no-default-features --features ' + device_features
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
  cargo +nightly fmt --all
  bun run format

gateway:
  bun run build -- --filter=@bridgething/gateway
  bun run gateway:example:dev

# --- Codegen ---

typescript:
  cargo run -q -p bridgething-codegen -- ts
  bun run format

swift:
  cargo run -q -p bridgething-codegen -- swift

kotlin:
  cargo run -q -p bridgething-codegen -- kotlin

rust:
  cargo run -q -p bridgething-codegen -- rust
  cargo +nightly fmt --all

codegen:
  cargo run -q -p bridgething-codegen -- all
  just fmt

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

# check out vendored submodules; a plain clone leaves swupdate's ipc sources empty and the cross image build dies
submodules:
  git submodule update --init --recursive

# Build the daemon build image. runs at the host's native arch and
# cross-compiles to aarch64, so it is emulation-free on both x86_64 and arm64.
build-image: submodules
  docker build -t bridgething-build -f scripts/cross-aarch64.Dockerfile .

# Cross-build the daemon for the Car Thing.
cross-build: build-image
  docker run --rm -v {{justfile_directory()}}:/work -w /work -v bridgething-cargo-registry:/usr/local/cargo/registry -e CARGO_TARGET_DIR=/work/target-cross -e RUSTFLAGS='--remap-path-prefix=/work=/bridgething --remap-path-prefix=/usr/local/cargo=/cargo' bridgething-build cargo build --release -p bridgething --target {{cross_target}} --no-default-features --features {{device_features}} {{dev_profile}}

cross-build-test: build-image
  docker run --rm -v {{justfile_directory()}}:/work -w /work -v bridgething-cargo-registry:/usr/local/cargo/registry -e CARGO_TARGET_DIR=/work/target-cross -e RUSTFLAGS='--remap-path-prefix=/work=/bridgething --remap-path-prefix=/usr/local/cargo=/cargo' bridgething-build cargo build --release -p bridgething --target {{cross_target}} --no-default-features --features "{{device_features}},test-tap" {{dev_profile}}

# Cross-check the daemon with the voice stack compiled in; `mic` is out of the shipping feature set
check-voice: build-image
  docker run --rm -v {{justfile_directory()}}:/work -w /work -v bridgething-cargo-registry:/usr/local/cargo/registry -e CARGO_TARGET_DIR=/work/target-cross bridgething-build cargo check -p bridgething --target {{cross_target}} --no-default-features --features "{{device_features}},mic" --locked

# Release-build the daemon inside the cross image. for any host without an aarch64 toolchain (mac).
cross-release: build-image
  docker run --rm -v {{justfile_directory()}}:/work -w /work -v bridgething-cargo-registry:/usr/local/cargo/registry -e CARGO_TARGET_DIR=/work/target-cross-release bridgething-build {{release_build}}

# Release-build the daemon against a toolchain already on the host, provisioned by
# scripts/cross-aarch64-deps.sh. this is the CI path; on a mac use cross-release instead.
cross-release-native:
  CARGO_TARGET_DIR={{cross_release_dir}} {{release_build}}

# Cross-build then push the daemon to /opt/bridgething/daemon/
push: cross-build
  scripts/bridgething-push-daemon {{cross_target_dir}}/{{cross_target}}/release/bridgething

# Cross-build the test-tap binary then push to /opt/bridgething/daemon/
push-test: cross-build-test
  scripts/bridgething-push-daemon {{cross_target_dir}}/{{cross_target}}/release/bridgething

# Cross-build the wake-word sidecar and push it + its graphs + its unit to the device
push-wakeword: build-image
  docker run --rm -v {{justfile_directory()}}:/work -w /work -v bridgething-cargo-registry:/usr/local/cargo/registry -e CARGO_TARGET_DIR=/work/target-cross bridgething-build cargo build --release -p bridgething-wakeword --bin bridgething-wakeword --target {{cross_target}} --locked
  scripts/bridgething-push-wakeword

# Pack the wake-word runtime tarball the image pins against
pack-wakeword *args:
  scripts/bridgething-pack-wakeword {{args}}

# Pack and publish the wake-word runtime + phrase model, printing the manifest fragment
publish-wakeword *args:
  scripts/bridgething-publish-wakeword {{args}}

# Cross-build the MFi i2c-3 dev proxy and push it + its unit to the device
push-mfi-proxy: build-image
  docker run --rm -v {{justfile_directory()}}:/work -w /work -v bridgething-cargo-registry:/usr/local/cargo/registry -e CARGO_TARGET_DIR=/work/target-cross bridgething-build cargo build --release -p bridgething-mfi-proxy --target {{cross_target}} --locked
  scripts/bridgething-push-mfi-proxy

# Push a webapp bundle into /var/bridgething/webapps/<name>/
push-webapp local name="":
  scripts/bridgething-push-webapp {{local}} {{name}}

# SSH into the device. Splits args - watch out for quoting
ssh *args:
  ssh {{ssh_args}} root@{{device_host}} {{args}}

# Tail bridgething.service journal.
logs:
  ssh {{ssh_args}} root@{{device_host}} journalctl -fu bridgething.service

# set the device's bridgething instance to trace
trace-dropin:
  ssh {{ssh_args}} root@{{device_host}} 'mkdir -p /etc/systemd/system/bridgething.service.d && echo -e "[Service]\nEnvironment=RUST_LOG=bridgething=trace,bridgething::ws::connection::send=debug,bridgething::net=debug,libbridgething=trace,bridgething_iap2=trace,bridgething_mfi=trace" > /etc/systemd/system/bridgething.service.d/zz-trace.conf && systemctl daemon-reload && systemctl restart bridgething.service'

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
