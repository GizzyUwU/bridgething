#!/usr/bin/env bash
set -euo pipefail

cd "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/.." # mobile root

echo "== gradle assembleRelease =="
( cd android && ./gradlew assembleRelease )

APK="android/app/build/outputs/apk/release/app-release.apk"
[ -f "$APK" ] || { echo "apk not found at $APK" >&2; exit 1; }
echo "done: mobile/$APK"
