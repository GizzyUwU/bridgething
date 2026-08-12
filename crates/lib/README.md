# @bridgething/lib

The wire-protocol layer for [bridgething](https://github.com/JoeyEamigh/bridgething),
the community daemon that replaces Spotify's stock app on the Car Thing. This
package holds the serialized types every peer speaks (companion phone, on-device
webapp, host tools). The codec and framing themselves live in the Rust crate this
package is generated from; a TypeScript consumer that needs to speak the byte
protocol uses [`@bridgething/browser`](https://www.npmjs.com/package/@bridgething/browser),
which is that crate compiled to wasm.

Most webapp authors want [`@bridgething/client`](https://www.npmjs.com/package/@bridgething/client)
instead, it's an ergonomic surface facade.

Subpath exports:

- `@bridgething/lib/client` / `/gateway` / `/stock` - per-protocol message types.
- `@bridgething/lib/shared` - types used across protocols (`Track`, `Album`, ...).
- `@bridgething/lib/wire` - the envelope (`MsgMeta`, `WireEvent`, `WireRequest`, ...).
- `@bridgething/lib/uuid` - the protocol UUIDs and id helpers.

## Learn more

- Full docs: <https://bridgething.com/docs>
- Scaffold a webapp: `bun create bridgething my-app`
- Source: <https://github.com/JoeyEamigh/bridgething>
