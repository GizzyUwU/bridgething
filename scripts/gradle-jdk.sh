#!/usr/bin/env bash
[ -n "${BASH_VERSION:-}" ] || {
  echo "gradle-jdk.sh must be sourced from bash" >&2
  return 1 2>/dev/null || exit 1
}

_gradle_jdk_usable() {
  local home="$1" major="$2" found

  [ -x "$home/bin/java" ] || return 1

  found="$(sed -n 's/^JAVA_VERSION="\{0,1\}\([0-9][0-9]*\).*/\1/p' "$home/release" 2>/dev/null | head -n1)"
  [ -n "$found" ] || found="$("$home/bin/java" -XshowSettings:properties -version 2>&1 \
    | sed -n 's/.*java\.specification\.version = \([0-9][0-9]*\).*/\1/p' | head -n1)"

  [ "$found" = "$major" ]
}

_gradle_jdk_find() {
  local major="$1" var root candidate
  local -a roots=() candidates=()

  for var in "JAVA_HOME_${major}_X64" "JAVA_HOME_${major}_ARM64" "JAVA_HOME_${major}"; do
    candidate="${!var:-}"
    if [ -n "$candidate" ] && _gradle_jdk_usable "$candidate" "$major"; then
      echo "$candidate"
      return 0
    fi
  done

  if [ -n "${GRADLE_USER_HOME:-}" ]; then
    roots+=("$GRADLE_USER_HOME")
  fi
  roots+=("$HOME/.local/share/gradle" "$HOME/.gradle")
  for root in "${roots[@]}"; do
    candidates+=("$root/jdks/"*"-${major}"* "$root/jdks/"*"-${major}"*/*)
  done

  candidates+=(
    "$HOME/Library/Application Support/javm/jdk/temurin@${major}.0/Contents/Home"
    "$HOME/.local/share/javm/jdk/"*"@${major}"*
    "/Library/Java/JavaVirtualMachines/temurin-${major}.jdk/Contents/Home"
    "$HOME/.sdkman/candidates/java/${major}"*
    "/usr/lib/jvm/java-${major}-openjdk"*
    "/usr/lib/jvm/temurin-${major}-jdk"*
    "/usr/lib/jvm/jdk-${major}"*
  )

  for candidate in "${candidates[@]}"; do
    if _gradle_jdk_usable "$candidate" "$major"; then
      echo "$candidate"
      return 0
    fi
  done

  return 1
}

gradle_jdk_env() {
  local jdk17 jdk21 fallback java_bin

  jdk21="$(_gradle_jdk_find 21 || true)"
  jdk17="$(_gradle_jdk_find 17 || true)"

  if [ -n "$jdk21" ]; then
    GRADLE_JAVA="$jdk21"
    GRADLE_INSTALLS="$jdk21"
    if [ -n "$jdk17" ]; then
      GRADLE_INSTALLS="$jdk17,$jdk21"
    fi
  else
    fallback="${JAVA_HOME:-}"
    if [ -z "$fallback" ] || [ ! -x "$fallback/bin/java" ]; then
      java_bin="$(command -v java || true)"
      [ -n "$java_bin" ] || { echo "no jdk found (install a jdk or set JAVA_HOME)" >&2; return 1; }
      fallback="$(dirname "$(dirname "$(readlink -f "$java_bin")")")"
    fi
    [ -x "$fallback/bin/java" ] || { echo "no usable jdk at $fallback (set JAVA_HOME)" >&2; return 1; }
    echo "no jdk 21 found, falling back to $fallback; toolchain resolution will reject it" >&2
    GRADLE_JAVA="$fallback"
    GRADLE_INSTALLS="$fallback"
  fi

  export GRADLE_JAVA GRADLE_INSTALLS
}
