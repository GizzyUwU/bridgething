run:
  cargo run -p bridgething

gatt:
  cd dev-btclient && bun run build

typeshare:
  typeshare ./core --lang=typescript --output-file=client/typescript/src/bridgething.ts