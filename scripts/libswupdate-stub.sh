#!/usr/bin/env bash
set -euo pipefail

SWUPDATE_SRC="${1:?usage: libswupdate-stub.sh <path to crates/swupdate-sys/vendor/swupdate> [cc] [libdir]}"
CC="${2:-cc}"
LIBDIR="${3:-/usr/lib}"

[ -f "$SWUPDATE_SRC/ipc/network_ipc.c" ] \
  || { echo "no swupdate ipc sources under $SWUPDATE_SRC (run: git submodule update --init --recursive)" >&2; exit 1; }

mkdir -p "$LIBDIR"
( cd "$SWUPDATE_SRC" && "$CC" -shared -fPIC \
    -Wl,-soname,libswupdate.so.0.1 \
    -I include \
    ipc/network_ipc.c ipc/network_ipc-if.c ipc/progress_ipc.c \
    -lpthread \
    -o "$LIBDIR/libswupdate.so.0.1" )
ln -sf libswupdate.so.0.1 "$LIBDIR/libswupdate.so"
