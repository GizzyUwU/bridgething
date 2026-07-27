#!/usr/bin/env bash
# provisions an aarch64-unknown-linux-gnu build environment: the link deps and the libswupdate
# stub, plus a cross toolchain when the host is not already aarch64. shared by
# cross-aarch64.Dockerfile and the CI daemon build so the container and the runner cannot drift
# into different environments.
#
# runs as root. takes the vendored swupdate checkout as its one argument.
#
# deps:
#   - libusb-1.0, libdbus-1, libasound2: bluer + alsa-sys link deps
#   - pkg-config: resolves those .pc files
#   - clang/libclang-dev: bindgen (swupdate-sys) needs libclang
#   - on a foreign host, additionally gcc-aarch64-linux-gnu (the linker for the rust target) and
#     libc6-dev-arm64-cross. the latter ships only as a *Recommends* of the former, so
#     --no-install-recommends drops it and the cross compiler cannot even #include <stdio.h>.
#
# License note: libswupdate is LGPL-2.1-or-later. The IPC sources are compiled here as an ephemeral
# build artifact; only the bridgething binary (which dynamically links the device's libswupdate.so
# on-target) is distributed. LGPL section 6 dynamic-linking provisions cover this.
set -euo pipefail

SWUPDATE_SRC="${1:?usage: cross-aarch64-deps.sh <path to crates/swupdate-sys/vendor/swupdate>}"
[ -f "$SWUPDATE_SRC/ipc/network_ipc.c" ] \
  || { echo "no swupdate ipc sources under $SWUPDATE_SRC (run: git submodule update --init --recursive)" >&2; exit 1; }

LIBDIR=/usr/lib/aarch64-linux-gnu

if [ "$(dpkg --print-architecture)" = arm64 ]; then
  # aarch64 host: the target is native, so the plain packages and the system gcc are the toolchain.
  CC=gcc
  PKGS=(libusb-1.0-0-dev libdbus-1-dev libasound2-dev pkg-config clang libclang-dev)
else
  # foreign host: pull the arm64 slice of each link dep alongside a cross compiler. pkg-config and
  # the clang tooling stay host-arch - they are build tools, not link targets. debian carries every
  # architecture on one mirror, which is why the build image is debian and not ubuntu.
  CC=aarch64-linux-gnu-gcc
  dpkg --add-architecture arm64
  PKGS=(
    gcc-aarch64-linux-gnu libc6-dev-arm64-cross
    libusb-1.0-0-dev:arm64 libdbus-1-dev:arm64 libasound2-dev:arm64
    pkg-config clang libclang-dev
  )
fi

apt-get update
apt-get install --assume-yes --no-install-recommends "${PKGS[@]}"
rm -rf /var/lib/apt/lists/*

# link-time stand-in built from the vendored IPC sources (pure libc + pthread, no Kconfig). it
# satisfies -lswupdate; on a real Car Thing the loader resolves the device's own libswupdate.so by
# SONAME, so this artifact is link-only and never shipped.
( cd "$SWUPDATE_SRC" && "$CC" -shared -fPIC \
    -Wl,-soname,libswupdate.so.0.1 \
    -I include \
    ipc/network_ipc.c ipc/network_ipc-if.c ipc/progress_ipc.c \
    -lpthread \
    -o "$LIBDIR/libswupdate.so.0.1" )
ln -sf libswupdate.so.0.1 "$LIBDIR/libswupdate.so"
