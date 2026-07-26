#!/usr/bin/env bash
set -euo pipefail

[ "$(uname -s)" = Darwin ] || { echo "testflight build requires macos" >&2; exit 1; }

cd "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/.." # mobile root

WORKSPACE="ios/bridgething.xcworkspace"
SCHEME="bridgething"
LOCAL_XCCONFIG="ios/Local.xcconfig"
ARCHIVE="ios/build/bridgething.xcarchive"
EXPORT_OPTS="ios/ExportOptions-appstore.plist"
IPA="ios/build/bridgething.ipa"

[ -f "$LOCAL_XCCONFIG" ] || { echo "no $LOCAL_XCCONFIG (copy Local.xcconfig.example and fill it in)" >&2; exit 1; }
TEAM_ID=$(sed -n 's/^[[:space:]]*DEVELOPMENT_TEAM[[:space:]]*=[[:space:]]*//p' "$LOCAL_XCCONFIG" | head -n1 | tr -d ' \r')
[ -n "$TEAM_ID" ] || { echo "$LOCAL_XCCONFIG sets no DEVELOPMENT_TEAM" >&2; exit 1; }

rm -rf "$ARCHIVE" "$IPA"

cat >"$EXPORT_OPTS" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>method</key>
  <string>app-store-connect</string>
  <key>signingStyle</key>
  <string>automatic</string>
  <key>teamID</key>
  <string>${TEAM_ID}</string>
  <key>destination</key>
  <string>export</string>
  <key>manageAppVersionAndBuildNumber</key>
  <false/>
  <key>uploadSymbols</key>
  <true/>
</dict>
</plist>
PLIST

VERSION_FLAGS=()
if [ -n "${BUILD_NUMBER:-}" ]; then
  VERSION_FLAGS=(--xcodebuild-flag=CURRENT_PROJECT_VERSION="$BUILD_NUMBER")
  echo "== build number: $BUILD_NUMBER =="
fi

if [ -z "${ASC_PRIVATE_KEY_PATH:-}" ] && [ -n "${ASC_OP_ITEM:-}" ] && command -v op >/dev/null 2>&1; then
  op_read() {
    if [ -n "${ASC_OP_ACCOUNT:-}" ]; then op read "$ASC_OP_ITEM/$1" --account "$ASC_OP_ACCOUNT" 2>/dev/null
    else op read "$ASC_OP_ITEM/$1" 2>/dev/null; fi
  }

  ASC_KEY_ID="${ASC_KEY_ID:-$(op_read keyId)}"
  ASC_ISSUER_ID="${ASC_ISSUER_ID:-$(op_read issuerId)}"
  [ -n "$ASC_KEY_ID" ] && [ -n "$ASC_ISSUER_ID" ] \
    || { echo "could not read keyId / issuerId from \$ASC_OP_ITEM" >&2; exit 1; }

  ASC_PRIVATE_KEY_PATH="/private/tmp/carthing-asc-$ASC_KEY_ID.p8"
  if [ ! -s "$ASC_PRIVATE_KEY_PATH" ]; then
    ( umask 077; op_read "AuthKey_$ASC_KEY_ID.p8" >"$ASC_PRIVATE_KEY_PATH" ) || true
    [ -s "$ASC_PRIVATE_KEY_PATH" ] \
      || { rm -f "$ASC_PRIVATE_KEY_PATH"; echo "op read of AuthKey_$ASC_KEY_ID.p8 failed" >&2; exit 1; }
    echo "== cached the asc key -> $ASC_PRIVATE_KEY_PATH (mode 0600, until reboot) =="
  fi
  export ASC_PRIVATE_KEY_PATH ASC_KEY_ID ASC_ISSUER_ID
fi

AUTH_FLAGS=()
if [ -n "${ASC_PRIVATE_KEY_PATH:-}" ]; then
  AUTH_FLAGS=(
    --xcodebuild-flag=-authenticationKeyPath --xcodebuild-flag="$ASC_PRIVATE_KEY_PATH"
    --xcodebuild-flag=-authenticationKeyID --xcodebuild-flag="${ASC_KEY_ID:?ASC_KEY_ID is required alongside ASC_PRIVATE_KEY_PATH}"
    --xcodebuild-flag=-authenticationKeyIssuerID --xcodebuild-flag="${ASC_ISSUER_ID:?ASC_ISSUER_ID is required alongside ASC_PRIVATE_KEY_PATH}"
  )
fi

echo "== asc xcode archive (signed, app-store) =="
asc xcode archive \
  --workspace "$WORKSPACE" \
  --scheme "$SCHEME" \
  --configuration Release \
  --archive-path "$ARCHIVE" \
  --clean --overwrite \
  --xcodebuild-flag=-destination --xcodebuild-flag=generic/platform=iOS \
  --xcodebuild-flag=-allowProvisioningUpdates \
  --xcodebuild-flag=CODE_SIGN_STYLE=Automatic \
  --xcodebuild-flag=DEVELOPMENT_TEAM="$TEAM_ID" \
  ${VERSION_FLAGS[@]+"${VERSION_FLAGS[@]}"} \
  ${AUTH_FLAGS[@]+"${AUTH_FLAGS[@]}"}

echo "== asc xcode export (app-store ipa) =="
asc xcode export \
  --archive-path "$ARCHIVE" \
  --export-options "$EXPORT_OPTS" \
  --ipa-path "$IPA" \
  --overwrite --timeout 15m \
  --xcodebuild-flag=-allowProvisioningUpdates \
  ${AUTH_FLAGS[@]+"${AUTH_FLAGS[@]}"}

echo "done: mobile/$IPA"
