# shellcheck shell=bash
host_dylib() {
  local target_dir="${CARGO_TARGET_DIR:-target}"
  case "$(uname -s)" in
    Darwin) echo "$target_dir/debug/libspotify.dylib" ;;
    Linux) echo "$target_dir/debug/libspotify.so" ;;
    *) echo "unsupported host os for binding generation: $(uname -s)" >&2; return 1 ;;
  esac
}
