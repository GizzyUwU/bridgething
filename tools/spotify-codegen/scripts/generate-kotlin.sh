#!/usr/bin/env bash
set -euo pipefail

# Runs openapi-generator-cli against the vendored sonallux spec to
# produce idiomatic Kotlin models + ktor-backed API stubs into the
# spotify-kotlin package source tree.
#
# Single source of truth lives at
# Sources/SpotifyOpenAPI/openapi.yaml. The Swift side reads the same
# file via the swift-openapi-generator build plugin (declared in
# Package.swift), so both languages stay aligned by construction.

cd "$(dirname "$0")/.."

bun x @openapitools/openapi-generator-cli generate \
  --config configs/kotlin.yaml
