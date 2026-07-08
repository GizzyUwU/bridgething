# @bridgething/lib

The wire-protocol layer for [bridgething](https://github.com/JoeyEamigh/bridgething),
the community daemon that replaces Spotify's stock app on the Car Thing. This
package holds the serialized types every peer speaks (companion phone, on-device
webapp, host tools) plus the codec and framing for the bridge and client links.

Most webapp authors want [`@bridgething/client`](https://www.npmjs.com/package/@bridgething/client)
instead, it's an ergonomic surface facade.

Subpath exports:

- `@bridgething/lib/client` / `/gateway` / `/stock` - per-protocol message types.
- `@bridgething/lib/shared` - types used across protocols (`Track`, `Album`, ...).
- `@bridgething/lib/wire` - the envelope (`MsgMeta`, `WireEvent`, `WireRequest`, ...).
- `@bridgething/lib/codec` / `/framing` - encode/decode and frame the byte stream.
- `@bridgething/lib/uuid` - the protocol UUIDs and id helpers.

## Learn more

- Full docs: <https://bridgething.com/docs>
- Scaffold a webapp: `bun create bridgething my-app`
- Source: <https://github.com/JoeyEamigh/bridgething>
