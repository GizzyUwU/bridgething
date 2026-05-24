# Custom cross image for `just push` aarch64 builds. Inherits cross's
# stock aarch64-unknown-linux-gnu image, then adds the target-arch
# C deps the bridgething daemon links against:
#
#   - libusb-1.0, libdbus-1, pkg-config: required by bluer transitive deps
#   - libasound2-dev: alsa-sys for the in-daemon mic capture surface
#   - libswupdate.so.0.1: built here from the vendored swupdate submodule
#     (just the IPC source files, no Kconfig - pure libc + pthread). The
#     resulting .so satisfies `-lswupdate` at link time. At runtime on a
#     real Car Thing the dynamic loader resolves to the device's full
#     libswupdate.so via SONAME match, so this build artifact is purely
#     a link-time stand-in and is never distributed.
#
# License note: libswupdate is LGPL-2.1-or-later. The IPC sources are
# compiled here as an ephemeral cross-build artifact baked into a local
# docker image; only the bridgething binary (which dynamically links
# against the device's libswupdate.so on-target) is distributed. LGPL
# §6 dynamic-linking provisions cover this cleanly.

ARG CROSS_BASE_IMAGE
FROM $CROSS_BASE_IMAGE

# Target-arch apt deps from the previous Cross.toml pre-build.
RUN dpkg --add-architecture arm64 && \
    apt-get update && \
    apt-get install --assume-yes --no-install-recommends \
      libusb-1.0-0-dev:arm64 \
      libdbus-1-dev:arm64 \
      libasound2-dev:arm64 \
      pkg-config:arm64 && \
    rm -rf /var/lib/apt/lists/*

# Compile libswupdate.so.0.1 from the vendored IPC source files. The
# COPY pulls from the workspace (Cross context = repo root). Three
# files, no Kconfig + no swupdate-side build deps - the IPC client lib
# is intentionally minimal.
COPY crates/swupdate-sys/vendor/swupdate /tmp/swupdate
RUN cd /tmp/swupdate && \
    aarch64-linux-gnu-gcc -shared -fPIC \
      -Wl,-soname,libswupdate.so.0.1 \
      -I include \
      ipc/network_ipc.c ipc/network_ipc-if.c ipc/progress_ipc.c \
      -lpthread \
      -o /usr/aarch64-linux-gnu/lib/libswupdate.so.0.1 && \
    ln -s libswupdate.so.0.1 /usr/aarch64-linux-gnu/lib/libswupdate.so && \
    rm -rf /tmp/swupdate
