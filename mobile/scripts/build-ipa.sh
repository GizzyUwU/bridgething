#!/usr/bin/env bash
set -euo pipefail

# xcodebuild only exists on macos.
[ "$(uname -s)" = Darwin ] || { echo "ipa build requires macos" >&2; exit 1; }

cd "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/.." # mobile root

WORKSPACE="ios/bridgething.xcworkspace"
SCHEME="bridgething"
ARCHIVE="ios/build/bridgething.xcarchive"
PAYLOAD="ios/build/Payload"
IPA="ios/build/bridgething.ipa"

echo "== xcodebuild archive (unsigned) =="
xcodebuild archive \
  -workspace "$WORKSPACE" \
  -scheme "$SCHEME" \
  -configuration Release \
  -destination 'generic/platform=iOS' \
  -archivePath "$ARCHIVE" \
  CODE_SIGNING_ALLOWED=NO CODE_SIGNING_REQUIRED=NO CODE_SIGN_IDENTITY=""

APP="$(ls -d "$ARCHIVE/Products/Applications/"*.app 2>/dev/null | head -1)"
[ -n "$APP" ] && [ -d "$APP" ] || { echo "archived .app not found under $ARCHIVE" >&2; exit 1; }

echo "== package unsigned ipa =="
rm -rf "$PAYLOAD" "$IPA"
mkdir -p "$PAYLOAD"
cp -R "$APP" "$PAYLOAD/"
( cd ios/build && zip -qry "bridgething.ipa" Payload )
rm -rf "$PAYLOAD"
echo "done: mobile/$IPA"
