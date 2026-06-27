#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/../../.." # bridgething workspace root
CRATE=crates/spotify
DYLIB=target/debug/libspotify.dylib
B="$CRATE/bindings"
: "${SPOTIFY_PRIVATE_STATE:=/tmp/sfp-live}"
export SPOTIFY_PRIVATE_STATE

echo "== build cdylib + regenerate bindings =="
cargo build -p spotify --lib
rm -rf "$B"
cargo run -q --bin uniffi-bindgen -- generate --library "$DYLIB" --language swift --out-dir "$B/swift"
cargo run -q --bin uniffi-bindgen -- generate --library "$DYLIB" --language kotlin --out-dir "$B/kotlin"
cp "$B/swift/spotifyFFI.modulemap" "$B/swift/module.modulemap"

echo "== swift harness =="
swiftc -O -I "$B/swift" -L target/debug -lspotify \
  "$B/swift/spotify.swift" "$CRATE/harness/swift/main.swift" \
  -o /tmp/sfp-swift-harness
DYLD_LIBRARY_PATH=target/debug /tmp/sfp-swift-harness

if [ -n "${KOTLINC:-}" ]; then
  echo "== kotlin harness =="
  "$KOTLINC" "$B/kotlin/uniffi/spotify/spotify.kt" "$CRATE/harness/kotlin/main.kt" \
    -cp "$JNA_JAR:$COROUTINES_JAR" -include-runtime -d /tmp/sfp-kotlin.jar
  "$JAVA_HOME/bin/java" -cp "/tmp/sfp-kotlin.jar:$JNA_JAR:$COROUTINES_JAR" \
    -Djna.library.path=target/debug MainKt
else
  echo "== kotlin harness skipped (set KOTLINC/JNA_JAR/COROUTINES_JAR/JAVA_HOME) =="
fi
