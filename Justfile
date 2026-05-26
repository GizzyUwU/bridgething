# bridgething Justfile

# --- Path config ---

cross_target := 'aarch64-unknown-linux-gnu'
cross_target_dir := justfile_directory() / 'target-cross'
device_host := env_var_or_default('SUPERBIRD_HOST', 'bridgething.local')
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

goldens:
  UPDATE_GOLDEN=1 cargo test -p libbridgething --test golden golden_vectors_match_fixture_file

# --- Device iteration ---

# Cross-build the daemon for the Car Thing.
cross-build:
  CARGO_TARGET_DIR={{cross_target_dir}} cross build --release -p bridgething --target {{cross_target}} --no-default-features --features superbird --config profile.release.lto=false --config profile.release.codegen-units=32

cross-build-test:
  CARGO_TARGET_DIR={{cross_target_dir}} cross build --release -p bridgething --target {{cross_target}} --no-default-features --features "superbird,test-tap" --config profile.release.lto=false --config profile.release.codegen-units=32

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
