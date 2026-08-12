#!/usr/bin/env bash
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

if [ ! -d .githooks ]; then
  echo "install.sh: no .githooks/ directory at repo root" >&2
  exit 1
fi

git config core.hooksPath .githooks
echo "installed: core.hooksPath -> .githooks (this clone only)"

if [ -f .githooks/pre-push ]; then
  chmod +x .githooks/pre-push
  echo "verified: .githooks/pre-push is executable"
fi
