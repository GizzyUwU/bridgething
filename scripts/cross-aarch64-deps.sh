#!/usr/bin/env bash
# provisions an aarch64-unknown-linux-gnu cross environment: the cross toolchain, the arm64
# multiarch link deps, and the link-time libswupdate stub. shared by cross-aarch64.Dockerfile and
# the CI daemon build so the container and the runner cannot drift into different environments.
#
# runs as root. takes the vendored swupdate checkout as its one argument.
#
# deps:
#   - gcc-aarch64-linux-gnu: the aarch64 linker for the rust target
#   - libc6-dev-arm64-cross: the aarch64 libc headers. shipped only as a *Recommends* of
#     gcc-aarch64-linux-gnu, so --no-install-recommends drops them and the cross compiler cannot
#     even #include <stdio.h>. must be named explicitly.
#   - libusb-1.0, libdbus-1, libasound2 (:arm64): bluer + alsa-sys link deps
#   - pkg-config + clang/libclang-dev: pkg-config resolves the arm64 .pc files; bindgen
#     (swupdate-sys) needs libclang
#
# License note: libswupdate is LGPL-2.1-or-later. The IPC sources are compiled here as an ephemeral
# build artifact; only the bridgething binary (which dynamically links the device's libswupdate.so
# on-target) is distributed. LGPL section 6 dynamic-linking provisions cover this.
set -euo pipefail

SWUPDATE_SRC="${1:?usage: cross-aarch64-deps.sh <path to crates/swupdate-sys/vendor/swupdate>}"
[ -f "$SWUPDATE_SRC/ipc/network_ipc.c" ] \
  || { echo "no swupdate ipc sources under $SWUPDATE_SRC (run: git submodule update --init --recursive)" >&2; exit 1; }

. /etc/os-release

# debian carries every arch on one mirror; ubuntu splits ports off onto its own host, so enabling
# arm64 without pinning the archs first makes apt-get update 404 on every arm64 index.
if [ "${ID:-}" = ubuntu ]; then
  if [ -f /etc/apt/sources.list.d/ubuntu.sources ]; then
    sed -i '/^Architectures:/d' /etc/apt/sources.list.d/ubuntu.sources
    sed -i '/^Types:/i Architectures: amd64' /etc/apt/sources.list.d/ubuntu.sources
    cat >/etc/apt/sources.list.d/ubuntu-ports-arm64.sources <<EOF
Types: deb
URIs: http://ports.ubuntu.com/ubuntu-ports
Suites: ${VERSION_CODENAME} ${VERSION_CODENAME}-updates ${VERSION_CODENAME}-security
Components: main universe
Architectures: arm64
Signed-By: /usr/share/keyrings/ubuntu-archive-keyring.gpg
EOF
  else
    sed -i 's/^deb \(\[[^]]*\] \)\?/deb [arch=amd64] /' /etc/apt/sources.list
    cat >/etc/apt/sources.list.d/ubuntu-ports-arm64.list <<EOF
deb [arch=arm64] http://ports.ubuntu.com/ubuntu-ports ${VERSION_CODENAME} main universe
deb [arch=arm64] http://ports.ubuntu.com/ubuntu-ports ${VERSION_CODENAME}-updates main universe
EOF
  fi
fi

dpkg --add-architecture arm64
apt-get update
apt-get install --assume-yes --no-install-recommends \
  gcc-aarch64-linux-gnu \
  libc6-dev-arm64-cross \
  libusb-1.0-0-dev:arm64 \
  libdbus-1-dev:arm64 \
  libasound2-dev:arm64 \
  pkg-config \
  clang \
  libclang-dev
rm -rf /var/lib/apt/lists/*

# link-time stand-in built from the vendored IPC sources (pure libc + pthread, no Kconfig). it
# satisfies -lswupdate; on a real Car Thing the loader resolves the device's own libswupdate.so by
# SONAME, so this artifact is link-only and never shipped.
( cd "$SWUPDATE_SRC" && aarch64-linux-gnu-gcc -shared -fPIC \
    -Wl,-soname,libswupdate.so.0.1 \
    -I include \
    ipc/network_ipc.c ipc/network_ipc-if.c ipc/progress_ipc.c \
    -lpthread \
    -o /usr/lib/aarch64-linux-gnu/libswupdate.so.0.1 )
ln -sf libswupdate.so.0.1 /usr/lib/aarch64-linux-gnu/libswupdate.so
