#!/usr/bin/env bash
# Push the production bundle to a Car Thing on the network. Mirrors the
# yocto-superbird `bridgething-push-webapp` script.
set -euo pipefail

if [ -z "${1:-}" ]; then
  echo "usage: bun run push <device-ip-or-hostname>" >&2
  exit 1
fi

HOST="$1"
DIST="dist"

if [ ! -d "$DIST" ]; then
  echo "no $DIST/ directory; run 'bun run build' first" >&2
  exit 1
fi

if [ ! -f "$DIST/manifest.json" ]; then
  echo "no $DIST/manifest.json; build appears incomplete" >&2
  exit 1
fi

UUID="$(node -e "console.log(JSON.parse(require('fs').readFileSync('$DIST/manifest.json','utf8')).id)")"
if [ -z "$UUID" ]; then
  echo "manifest.json missing 'id' field" >&2
  exit 1
fi

# Ssh args: keys regenerate on every flash, so skip host-key checking.
SSH_OPTS=(-o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o LogLevel=ERROR)

echo "rsync $DIST/ to root@$HOST:/var/bridgething/webapps/$UUID/"
rsync -az --delete \
  -e "ssh ${SSH_OPTS[*]}" \
  "$DIST/" \
  "root@$HOST:/var/bridgething/webapps/$UUID/"

echo "done. the daemon will pick up the new bundle on its next registry rescan; switch via the gateway companion."
