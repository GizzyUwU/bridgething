run:
  cargo run -p bridgething

build:
  just typescript
  cargo build
  bun run build

adapter:
  cargo run --example bridgething-adapter-gateway

gatt:
  bun run build
  bun run dev:gateway

typescript:
  rm -rf lib/ts/bindings
  cargo test -p libbridgething &> /dev/null || exit 0
  bunx prettier lib/ts/bindings --write