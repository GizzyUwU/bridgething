#!/usr/bin/env bash
[ -n "${BASH_VERSION:-}" ] || {
  echo "rn-prebuilt-markers.sh must be sourced from bash" >&2
  return 1 2>/dev/null || exit 1
}

reset_rn_prebuilt_markers() {
  local marker
  for marker in \
    ios/Pods/.last_build_configuration \
    ios/Pods/ReactNativeDependencies/.last_build_configuration \
    ios/Pods/React-Core-prebuilt/.last_build_configuration
  do
    if [ -d "$(dirname "$marker")" ]; then printf Release >"$marker"; fi
  done
  return 0
}
