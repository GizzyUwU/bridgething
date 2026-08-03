# shellcheck shell=bash
host_dylib() {
  local crate="$1"
  local target_dir="${CARGO_TARGET_DIR:-target}"
  case "$(uname -s)" in
    Darwin) echo "$target_dir/debug/lib$crate.dylib" ;;
    Linux) echo "$target_dir/debug/lib$crate.so" ;;
    *) echo "unsupported host os for binding generation: $(uname -s)" >&2; return 1 ;;
  esac
}
