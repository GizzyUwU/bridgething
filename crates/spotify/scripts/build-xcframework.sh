#!/usr/bin/env bash
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$HERE/common.sh"

# xcframework assembly needs the apple toolchain (xcodebuild, lipo), which only ships on macos.
[ "$(uname -s)" = Darwin ] || { echo "ios xcframework build requires macos" >&2; exit 1; }

cd "$HERE/../../.." # bridgething workspace root
CRATE=crates/spotify
PKG=packages/spotify/swift
LIB=libspotify.a
PROFILE=release

DEVICE=aarch64-apple-ios
SIM_ARM=aarch64-apple-ios-sim
SIM_X86=x86_64-apple-ios
MAC_ARM=aarch64-apple-darwin
MAC_X86=x86_64-apple-darwin

TARGET_DIR="${CARGO_TARGET_DIR:-target}"

# an archive for generic/platform=iOS never resolves the simulator or macos slices, so a release
# build can skip four staticlibs and the fat-lto link each one ends in.
if [ "${XCFRAMEWORK_DEVICE_ONLY:-0}" = "1" ]; then
  TARGETS=("$DEVICE")
else
  TARGETS=("$DEVICE" "$SIM_ARM" "$SIM_X86" "$MAC_ARM" "$MAC_X86")
fi

XCF="$PKG/Frameworks/SpotifyFFI.xcframework"
SWIFT_OUT="$PKG/Sources/Spotify"
WORK="$CRATE/build"
HDRS="$WORK/headers"

echo "== rustup targets =="
rustup target add "${TARGETS[@]}" >/dev/null
rustup component add llvm-tools >/dev/null 2>&1 || true
OBJCOPY="$(rustc --print sysroot)/lib/rustlib/$(rustc -vV | sed -n 's/host: //p')/bin/rust-objcopy"

echo "== generate swift bindings =="
cargo build -q -p spotify --lib
rm -rf "$WORK"; mkdir -p "$WORK/gen" "$HDRS"
cargo run -q --bin uniffi-bindgen -- generate --library "$(host_dylib)" --language swift --out-dir "$WORK/gen"
mkdir -p "$SWIFT_OUT"
cp "$WORK/gen/spotify.swift" "$SWIFT_OUT/spotify.swift"
cp "$WORK/gen/spotifyFFI.h" "$HDRS/spotifyFFI.h"
# create-xcframework wants the modulemap named module.modulemap in the headers dir.
cp "$WORK/gen/spotifyFFI.modulemap" "$HDRS/module.modulemap"

echo "== build staticlibs (release) =="
export IPHONEOS_DEPLOYMENT_TARGET=18.0
export MACOSX_DEPLOYMENT_TARGET=15.0
for t in "${TARGETS[@]}"; do
  cargo rustc -q -p spotify --lib --crate-type staticlib --"$PROFILE" --target "$t"
  "$OBJCOPY" --remove-section=__TEXT,__eh_frame --remove-section=__LD,__compact_unwind "$TARGET_DIR/$t/$PROFILE/$LIB"
done

xcf_args=(-library "$TARGET_DIR/$DEVICE/$PROFILE/$LIB" -headers "$HDRS")

if [ "${#TARGETS[@]}" -gt 1 ]; then
  echo "== lipo simulator + macos arches =="
  SIM_FAT="$WORK/sim/$LIB"
  MAC_FAT="$WORK/mac/$LIB"
  mkdir -p "$WORK/sim" "$WORK/mac"
  lipo -create "$TARGET_DIR/$SIM_ARM/$PROFILE/$LIB" "$TARGET_DIR/$SIM_X86/$PROFILE/$LIB" -output "$SIM_FAT"
  lipo -create "$TARGET_DIR/$MAC_ARM/$PROFILE/$LIB" "$TARGET_DIR/$MAC_X86/$PROFILE/$LIB" -output "$MAC_FAT"
  xcf_args+=(-library "$SIM_FAT" -headers "$HDRS" -library "$MAC_FAT" -headers "$HDRS")
fi

echo "== assemble xcframework =="
rm -rf "$XCF"; mkdir -p "$PKG/Frameworks"
xcodebuild -create-xcframework "${xcf_args[@]}" -output "$XCF"

echo "done: $XCF + $SWIFT_OUT/spotify.swift"
