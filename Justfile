run:
  cargo run -p bridgething

build:
  just typescript
  cargo build
  bun run build

gateway:
  bun run build -- --filter=@bridgething/gateway
  bun run gateway:example:dev

typescript:
  rm -rf lib/ts/bindings
  cargo test -p libbridgething &> /dev/null || exit 0
  bunx prettier lib/ts/bindings --write