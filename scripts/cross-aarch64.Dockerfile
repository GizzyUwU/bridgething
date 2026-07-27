# build image for the Car Thing daemon. runs at the HOST's native arch (no
# --platform pin) and cross-compiles to aarch64-unknown-linux-gnu, so it is
# native-speed with zero emulation on both x86_64 and arm64 hosts. on an arm64
# host the aarch64 toolchain is effectively native. a prior variant pinned
# --platform arm64 and forced slow emulation on x86 hosts; this avoids that.
#
# the cross environment itself is provisioned by scripts/cross-aarch64-deps.sh, which the CI daemon
# build runs directly against the runner. sharing it is what keeps the container and the runner from
# becoming two different build environments.

FROM rust:1.94-bookworm

# chromium is the host-arch browser the dev-linux-host `chrome` feature drives; it is not needed to
# link the device build, only to run the daemon inside this image.
RUN apt-get update && \
    apt-get install --assume-yes --no-install-recommends chromium && \
    rm -rf /var/lib/apt/lists/*

COPY scripts/cross-aarch64-deps.sh /tmp/cross-aarch64-deps.sh
COPY crates/swupdate-sys/vendor/swupdate /tmp/swupdate
RUN /tmp/cross-aarch64-deps.sh /tmp/swupdate && rm -rf /tmp/swupdate /tmp/cross-aarch64-deps.sh

RUN rustup target add aarch64-unknown-linux-gnu

ENV CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc \
    PKG_CONFIG_ALLOW_CROSS=1 \
    PKG_CONFIG_PATH=/usr/lib/aarch64-linux-gnu/pkgconfig
