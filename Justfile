# bridgething Justfile
#
# Two flavors of recipes:
#   1. Local dev - `cargo run`, `just codegen`, gateway/adapter builds.
#   2. Device iteration - cross-build the daemon and push to a Car Thing
#      over USB-CDC-ECM. Helper scripts live in scripts/. Host defaults
#      to 10.42.1.2 (the gadget end of the USB-CDC link); override with
#      SUPERBIRD_HOST.

# --- Path config ---

# cross-rs target tuple. Car Thing userspace runs aarch64 glibc.
cross_target := 'aarch64-unknown-linux-gnu'

# Separate target dir for cross builds. Mixing host build-script ELFs
# (built against host glibc) with cross-built ones (built against the
# Ubuntu 20.04 glibc inside cross's container) silently breaks: a build
# script compiled for host glibc cannot execute inside the container
# and the build dies with a GLIBC version mismatch. Isolating the
# target dir is the cheapest fix and beats `cargo clean` before every
# cross invocation.
cross_target_dir := justfile_directory() / 'target-cross'

# Default device host. The Car Thing exposes itself as a USB-CDC-ECM
# gadget at 10.42.1.2 when plugged in.
device_host := env_var_or_default('SUPERBIRD_HOST', '10.42.1.2')

# ssh args used by every recipe that talks to the device. Overrides
# host-key checking because the device's keys regenerate on each flash.
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

adapter:
  bun run build -- --filter=@bridgething/adapter-node

# --- Codegen ---

typescript:
  cargo run -q -p bridgething-codegen -- ts

swift:
  cargo run -q -p bridgething-codegen -- swift

kotlin:
  cargo run -q -p bridgething-codegen -- kotlin

codegen:
  cargo run -q -p bridgething-codegen -- all

goldens:
  UPDATE_GOLDEN=1 cargo test -p libbridgething --test golden golden_vectors_match_fixture_file

# --- Device iteration ---

# Cross-build the daemon for the Car Thing. The `superbird` feature
# flag selects the on-device build (sd-notify, ALS, mic, chromium CDP
# wired up; dev-host features dropped).
cross-build:
  CARGO_TARGET_DIR={{cross_target_dir}} cross build --release -p bridgething --target {{cross_target}} --no-default-features --features superbird --config profile.release.lto=false --config profile.release.codegen-units=32

# Cross-build then push the daemon to /opt/bridgething/daemon/ on the
# device. The push script stops bridgething + bridgething-weston first
# (chromium-kiosk cascades) so the 17 MB transfer over USB-CDC doesn't
# OOM the running stack, then restarts both. /opt/bridgething is a
# bind-mount from the settings partition so the dropped binary
# survives bootslot swaps and OTA upgrades.
push: cross-build
  scripts/bridgething-push-daemon {{cross_target_dir}}/{{cross_target}}/release/bridgething

# Push a webapp bundle into /var/bridgething/webapps/<name>/. Default
# name is the basename of <local>. Daemon picks it up next time
# Webapps::SwitchTo names it (or on next boot if active webapp's
# manifest already points there).
push-webapp local name="":
  scripts/bridgething-push-webapp {{local}} {{name}}

# SSH into the device. Pass commands through as positional args.
ssh *args:
  ssh {{ssh_args}} root@{{device_host}} {{args}}

# Tail bridgething.service journal. Ctrl-C to stop.
logs:
  ssh {{ssh_args}} root@{{device_host}} journalctl -fu bridgething.service

# Tunnel chromium's CDP socket from the device's 127.0.0.1:9222 to
# the host. Required because chromium >= M111 silently ignores
# --remote-debugging-address=non-localhost; this tunnel is what makes
# chrome://inspect see the kiosk.
cdp port="9222":
  scripts/bridgething-cdp {{port}}

# Build, push, and tail logs in one shot. The dev iteration loop:
# edit, run this, watch the daemon come up under journalctl, Ctrl-C
# when you want to edit again. Recipe dependencies handle the build
# + push; the tail blocks until you exit.
iter: push logs

# --- MFi dev proxy (dev-image only) ---
#
# bridgething-mfi-proxy listens on 10.42.1.2:9090 and forwards i2c-3
# transactions to the chip so `cargo test -p bridgething-mfi --test
# remote -- --ignored` (or any RemoteI2c-using tool) can drive the chip
# from the dev host. The proxy unit Conflicts= with bridgething.service
# and bridgething-als.service — starting it stops both, and blanks the
# backlight on entry / restores it on exit.

# Stop bridgething + ALS (via systemd Conflicts=) and start the i2c-3
# proxy. Backlight goes to zero. Use mfi-proxy-down to reverse.
mfi-proxy-up:
  ssh {{ssh_args}} root@{{device_host}} 'systemctl start bridgething-mfi-proxy.service'

# Stop the proxy and bring bridgething + ALS back up. Backlight restores
# to whatever it was before the proxy started (then ALS takes over).
mfi-proxy-down:
  ssh {{ssh_args}} root@{{device_host}} 'systemctl stop bridgething-mfi-proxy.service; systemctl start bridgething-als.service bridgething.service'

# Tail the proxy's journal. Ctrl-C to stop.
mfi-proxy-logs:
  ssh {{ssh_args}} root@{{device_host}} journalctl -fu bridgething-mfi-proxy.service

# --- Misc ---

# Set the host bluetooth class to the Car Thing class (0x7c0000) so
# stock-webapp and gateway pairing flows behave the same as on real
# hardware. Run once after each adapter restart.
class:
  sudo hciconfig hci0 class 0x7c0000 || true
  sudo hciconfig hci1 class 0x7c0000 || true
  sudo hciconfig hci2 class 0x7c0000 || true

tokei:
  tokei -t Nix,Rust,TypeScript,TSX,JavaScript
