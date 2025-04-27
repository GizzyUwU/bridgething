run:
  cargo run -p bridgething

build:
  just typescript
  cargo build
  bun run build

gateway:
  bun run build -- --filter=@bridgething/gateway
  bun run gateway:example:dev

adapter:
  bun run build -- --filter=@bridgething/adapter-node

typescript:
  rm -rf lib/ts/bindings
  cargo test -p libbridgething &> /dev/null
  bunx prettier lib/ts/bindings --write

class:
  sudo hciconfig hci0 class 0x7c0000 || true
  sudo hciconfig hci1 class 0x7c0000 || true
  sudo hciconfig hci2 class 0x7c0000 || true

tokei:
  tokei -t Nix,Rust,TypeScript,TSX,JavaScript