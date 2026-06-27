# shellcheck shell=bash
host_dylib() {
  case "$(uname -s)" in
    Darwin) echo "target/debug/libspotify.dylib" ;;
    Linux) echo "target/debug/libspotify.so" ;;
    *) echo "unsupported host os for binding generation: $(uname -s)" >&2; return 1 ;;
  esac
}
