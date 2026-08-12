#!/usr/bin/env bash
set -euo pipefail

cd "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/.."

case "${1:-release}" in
release)
    TASK=assembleRelease
    ABI=arm64-v8a
    APK="android/app/build/outputs/apk/release/app-release.apk"
    ;;
emulator)
    TASK=assembleDebug
    ABI=x86_64
    APK="android/app/build/outputs/apk/debug/app-debug.apk"
    ;;
*)
    echo "usage: build-apk.sh [release|emulator]" >&2
    exit 2
    ;;
esac

source ../scripts/gradle-jdk.sh
gradle_jdk_env

command -v bun >/dev/null || { echo "bun not found (required for nitro:codegen)" >&2; exit 1; }
echo "== nitro:codegen =="
( cd .. && bunx turbo run nitro:codegen )

echo "== gradle $TASK ($ABI, jdk: $GRADLE_JAVA) =="
( cd android && JAVA_HOME="$GRADLE_JAVA" ./gradlew "$TASK" \
    --no-daemon --console=plain --stacktrace \
    -PreactNativeArchitectures="$ABI" \
    -PcargoNdkAbis="$ABI" \
    -Porg.gradle.java.installations.paths="$GRADLE_INSTALLS" \
    -Porg.gradle.java.installations.auto-download=false </dev/null )

[ -f "$APK" ] || { echo "apk not found at $APK" >&2; exit 1; }
echo "done: mobile/$APK"
