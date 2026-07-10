#!/usr/bin/env bash
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$HERE/common.sh"

cd "$HERE/../../.." # bridgething workspace root
MODULE=packages/spotify/kotlin/spotify
JNILIBS="$MODULE/src/main/jniLibs"
KOTLIN_OUT="$MODULE/src/main/kotlin"

# locate the android ndk across mac and linux: explicit env first, then common sdk roots.
find_ndk() {
  for explicit in "${ANDROID_NDK_HOME:-}" "${ANDROID_NDK_ROOT:-}" "${ANDROID_NDK:-}"; do
    [ -n "$explicit" ] && [ -d "$explicit" ] && { echo "$explicit"; return 0; }
  done
  local roots=(
    "${ANDROID_SDK_ROOT:-}"
    "${ANDROID_HOME:-}"
    "$HOME/Android/Sdk"
    "$HOME/Library/Android/sdk"
    "/opt/android-sdk"
    "/usr/local/lib/android/sdk"
    "/opt/homebrew/share/android-commandlinetools"
  )
  for root in "${roots[@]}"; do
    [ -n "$root" ] && [ -d "$root/ndk" ] || continue
    local latest
    latest="$(ls -1 "$root/ndk" 2>/dev/null | sort -V | tail -1)"
    [ -n "$latest" ] && [ -d "$root/ndk/$latest" ] && { echo "$root/ndk/$latest"; return 0; }
  done
  for bundle in "${ANDROID_SDK_ROOT:-}/ndk-bundle" "${ANDROID_HOME:-}/ndk-bundle" /opt/android-ndk; do
    [ -d "$bundle" ] && { echo "$bundle"; return 0; }
  done
  return 1
}

ANDROID_NDK_HOME="$(find_ndk)" || { echo "no android ndk found (set ANDROID_NDK_HOME)" >&2; exit 1; }
export ANDROID_NDK_HOME
echo "== ndk: $ANDROID_NDK_HOME =="

echo "== rustup android targets =="
# arm64-v8a only: arm32 is EOL and x86/x86_64 are emulator-only; no real phone needs them.
rustup target add aarch64-linux-android >/dev/null

echo "== generate kotlin bindings =="
cargo build -q -p spotify --lib
cargo run -q --bin uniffi-bindgen -- generate \
  --library "$(host_dylib)" \
  --language kotlin --out-dir "$KOTLIN_OUT"

echo "== build jniLibs (release, arm64-v8a) =="
rm -rf "$JNILIBS"; mkdir -p "$JNILIBS"
cargo ndk \
  -t arm64-v8a \
  -o "$JNILIBS" \
  build --release -p spotify --lib

echo "done: $JNILIBS + $KOTLIN_OUT/uniffi/spotify/"
