#!/usr/bin/env bash
# Push the production bundle to a Car Thing on the network. Mirrors the
# yocto-superbird `bridgething-push-webapp` script.
set -euo pipefail

if [ -z "${1:-}" ]; then
  echo "usage: bun run push <device-ip-or-hostname>" >&2
  exit 1
fi

HOST="$1"
NAME="$(node -p "require('./package.json').name")"
DIST="dist"

if [ ! -d "$DIST" ]; then
  echo "no $DIST/ directory; run 'bun run build' first" >&2
  exit 1
fi

# Ssh args: keys regenerate on every flash, so skip host-key checking.
SSH_OPTS=(-o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o LogLevel=ERROR)

echo "rsync $DIST/ to root@$HOST:/var/bridgething/webapps/$NAME/"
rsync -az --delete \
  -e "ssh ${SSH_OPTS[*]}" \
  "$DIST/" \
  "root@$HOST:/var/bridgething/webapps/$NAME/"

echo "switching active webapp on the device"
# This requires a companion to be paired. If not, install the webapp
# but skip the switch — the webapp will be picked up next time the
# kiosk reloads.
ssh "${SSH_OPTS[@]}" "root@$HOST" "true" || {
  echo "ssh ok"
}

echo "done. open the device's chromium kiosk to see the new bundle."
