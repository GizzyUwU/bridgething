#!/usr/bin/env bash
set -euo pipefail

SWUPDATE_SRC="${1:?usage: cross-aarch64-deps.sh <path to crates/swupdate-sys/vendor/swupdate>}"
[ -f "$SWUPDATE_SRC/ipc/network_ipc.c" ] \
  || { echo "no swupdate ipc sources under $SWUPDATE_SRC (run: git submodule update --init --recursive)" >&2; exit 1; }

LIBDIR=/usr/lib/aarch64-linux-gnu

if [ "$(dpkg --print-architecture)" = arm64 ]; then
  CC=gcc
  PKGS=(libusb-1.0-0-dev libdbus-1-dev libasound2-dev pkg-config clang libclang-dev cmake)
else
  CC=aarch64-linux-gnu-gcc
  dpkg --add-architecture arm64
  PKGS=(
    gcc-aarch64-linux-gnu libc6-dev-arm64-cross
    libusb-1.0-0-dev:arm64 libdbus-1-dev:arm64 libasound2-dev:arm64
    pkg-config clang libclang-dev cmake
  )
fi

apt-get update
apt-get install --assume-yes --no-install-recommends "${PKGS[@]}"
rm -rf /var/lib/apt/lists/*

( cd "$SWUPDATE_SRC" && "$CC" -shared -fPIC \
    -Wl,-soname,libswupdate.so.0.1 \
    -I include \
    ipc/network_ipc.c ipc/network_ipc-if.c ipc/progress_ipc.c \
    -lpthread \
    -o "$LIBDIR/libswupdate.so.0.1" )
ln -sf libswupdate.so.0.1 "$LIBDIR/libswupdate.so"
