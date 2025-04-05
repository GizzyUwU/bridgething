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

tokei:
  tokei -t Nix,Rust,TypeScript,TSX