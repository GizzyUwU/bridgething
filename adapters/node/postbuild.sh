#!/bin/bash
set -euo pipefail

if [ -f "./dist/index.d.ts" ]; then
  IMPORT_STATEMENT="import { BridgeToGatewayMsg, GatewayToBridgeMsg } from '@bridgething/lib';\n"
  awk -v imp="$IMPORT_STATEMENT" 'BEGIN {print imp} {print}' "./dist/index.d.ts" >temp_file && mv temp_file "./dist/index.d.ts"
  echo "added imports to ./dist/index.d.ts"
else
  echo "error: ./dist/index.d.ts not found"
  exit 1
fi
