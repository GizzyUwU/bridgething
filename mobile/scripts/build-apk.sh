#!/usr/bin/env bash
set -euo pipefail

cd "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/.." # mobile root

JAVM="$HOME/Library/Application Support/javm/jdk"
JDK17="$JAVM/temurin@17.0/Contents/Home"
JDK21="$JAVM/temurin@21.0/Contents/Home"
for jdk in "$JDK17" "$JDK21"; do
  [ -x "$jdk/bin/java" ] || { echo "missing jdk (install via javm): $jdk" >&2; exit 1; }
done

echo "== gradle assembleRelease =="
# rn settings-plugin needs jdk 17, the rest 21; gradle 9 foojay auto-download is broken so pin both and disable it.
( cd android && JAVA_HOME="$JDK21" ./gradlew assembleRelease \
    -Porg.gradle.java.installations.paths="$JDK17,$JDK21" \
    -Porg.gradle.java.installations.auto-download=false )

APK="android/app/build/outputs/apk/release/app-release.apk"
[ -f "$APK" ] || { echo "apk not found at $APK" >&2; exit 1; }
echo "done: mobile/$APK"
