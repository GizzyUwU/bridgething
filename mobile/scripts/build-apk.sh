#!/usr/bin/env bash
set -euo pipefail

cd "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/.." # mobile root

# JDK selection: prefer the macOS javm layout; otherwise fall back to JAVA_HOME
# or the system `java` so Linux (lycaon / CI) can build too.
JAVM="$HOME/Library/Application Support/javm/jdk"
JDK17="$JAVM/temurin@17.0/Contents/Home"
JDK21="$JAVM/temurin@21.0/Contents/Home"
if [ -x "$JDK17/bin/java" ] && [ -x "$JDK21/bin/java" ]; then
  GRADLE_JAVA="$JDK21"
  INSTALLS="$JDK17,$JDK21"
else
  GRADLE_JAVA="${JAVA_HOME:-}"
  if [ -z "$GRADLE_JAVA" ] || [ ! -x "$GRADLE_JAVA/bin/java" ]; then
    j="$(command -v java || true)"
    [ -n "$j" ] || { echo "no jdk found (install a JDK or set JAVA_HOME)" >&2; exit 1; }
    GRADLE_JAVA="$(dirname "$(dirname "$(readlink -f "$j")")")"
  fi
  [ -x "$GRADLE_JAVA/bin/java" ] || { echo "no usable jdk at $GRADLE_JAVA (set JAVA_HOME)" >&2; exit 1; }
  INSTALLS="$GRADLE_JAVA"
fi

echo "== gradle assembleRelease (jdk: $GRADLE_JAVA) =="
# rn settings-plugin prefers jdk 17, the rest 21; gradle 9 foojay auto-download is broken so pin installs and disable it.
( cd android && JAVA_HOME="$GRADLE_JAVA" ./gradlew assembleRelease \
    -Porg.gradle.java.installations.paths="$INSTALLS" \
    -Porg.gradle.java.installations.auto-download=false )

APK="android/app/build/outputs/apk/release/app-release.apk"
[ -f "$APK" ] || { echo "apk not found at $APK" >&2; exit 1; }
echo "done: mobile/$APK"
