# build image for the Car Thing daemon. runs at the HOST's native arch (no
# --platform pin) and cross-compiles to aarch64-unknown-linux-gnu, so it is
# native-speed with zero emulation on both x86_64 and arm64 hosts. on an arm64
# host the aarch64 toolchain is effectively native. a prior variant pinned
# --platform arm64 and forced slow emulation on x86 hosts; this avoids that.
#
# deps:
#   - gcc-aarch64-linux-gnu: the aarch64 linker for the rust target
#   - libc6-dev-arm64-cross: the aarch64 libc headers. debian ships these as a
#     *Recommends* of gcc-aarch64-linux-gnu, so --no-install-recommends drops
#     them and the cross compiler cannot even #include <stdio.h>. must be named
#     explicitly or the libswupdate stub below fails on bits/libc-header-start.h.
#   - libusb-1.0, libdbus-1, libasound2 (:arm64): bluer + alsa-sys link deps
#   - pkg-config + clang/libclang-dev: pkg-config resolves the arm64 .pc files;
#     bindgen (swupdate-sys) needs libclang
#   - libswupdate.so.0.1: link-time stand-in built from the vendored IPC
#     sources (pure libc + pthread, no Kconfig). satisfies `-lswupdate` at link
#     time; on a real Car Thing the loader resolves the device's full
#     libswupdate.so by SONAME, so this artifact is link-only and never shipped.
#
# License note: libswupdate is LGPL-2.1-or-later. The IPC sources are compiled
# here as an ephemeral build artifact baked into a local docker image; only the
# bridgething binary (which dynamically links the device's libswupdate.so
# on-target) is distributed. LGPL §6 dynamic-linking provisions cover this.

FROM rust:1.94-bookworm

RUN dpkg --add-architecture arm64 && \
    apt-get update && \
    apt-get install --assume-yes --no-install-recommends \
      gcc-aarch64-linux-gnu \
      libc6-dev-arm64-cross \
      libusb-1.0-0-dev:arm64 \
      libdbus-1-dev:arm64 \
      libasound2-dev:arm64 \
      pkg-config \
      clang \
      libclang-dev \
      chromium && \
    rm -rf /var/lib/apt/lists/*

RUN rustup target add aarch64-unknown-linux-gnu

COPY crates/swupdate-sys/vendor/swupdate /tmp/swupdate
RUN cd /tmp/swupdate && \
    aarch64-linux-gnu-gcc -shared -fPIC \
      -Wl,-soname,libswupdate.so.0.1 \
      -I include \
      ipc/network_ipc.c ipc/network_ipc-if.c ipc/progress_ipc.c \
      -lpthread \
      -o /usr/lib/aarch64-linux-gnu/libswupdate.so.0.1 && \
    ln -s libswupdate.so.0.1 /usr/lib/aarch64-linux-gnu/libswupdate.so && \
    rm -rf /tmp/swupdate

ENV CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc \
    PKG_CONFIG_ALLOW_CROSS=1 \
    PKG_CONFIG_PATH=/usr/lib/aarch64-linux-gnu/pkgconfig
