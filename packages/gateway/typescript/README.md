# @bridgething/gateway

Typed, codegen-driven facade for the bridgething daemon's gateway wire
protocol. One class (`BridgethingGateway`) over a swappable byte-level
`Adapter`, with surfaces (`gateway.player.*`, `gateway.webapp.*`,
`gateway.system.*`, …) emitted by the codegen tool from the canonical
Rust types in `crates/lib`.

## Adapters

The gateway is transport-agnostic. Pick the adapter that matches the
host:

| Host                            | Adapter                                                       |
| ------------------------------- | ------------------------------------------------------------- |
| Browser (Chrome 117+ / 138+)    | `WebSerialAdapter` from `@bridgething/gateway`                |
| Node / Bun / Deno / browser TCP | `NetworkAdapter` from `@bridgething/adapter-network`          |
| iOS native                      | `BridgethingGateway` Swift package (see `packages/companion`) |
| Android native                  | `BridgethingGateway` Kotlin package (see `packages/companion`) |
| React Native                    | `@bridgething/adapter-react-native`                           |

There is no host-side native BT path any more. The retired
`@bridgething/adapter-node` (NAPI + bluer/btleplug) was replaced by
`@bridgething/adapter-network`, which speaks the daemon's network gateway
WebSocket on port 8892. Plug a Car Thing in over USB and the dev image
exposes it at `ws://bridgething.local:8892/`.

## Quick start — browser

```ts
import { BridgethingGateway, WebSerialAdapter } from '@bridgething/gateway';

const adapter = new WebSerialAdapter();
const gateway = new BridgethingGateway(adapter);

gateway.on(event => {
  if (event.type === 'connected') console.log('up', event.device);
});

gateway.version.on((deviceId, meta) => console.log('meta:', meta));

await gateway.start();
// Then, in response to a user gesture:
await adapter.requestDevice();
```

## Quick start — Node

```ts
import { BridgethingGateway } from '@bridgething/gateway';
import { NetworkAdapter } from '@bridgething/adapter-network';

const gateway = new BridgethingGateway(new NetworkAdapter());
gateway.on(event => console.log(event));
await gateway.start(); // dials ws://bridgething.local:8892/
```

## Typed surfaces

Every method on `gateway.<surface>.<verb>(...)` is generated from the
Rust wire types and round-trips back to a typed response. See
`src/dispatch.generated.ts` for the full inventory. Examples:

```ts
const list = await gateway.webapp.list(deviceId);          // TypedRequestResult<WebappList, never>
const ack  = await gateway.webapp.installBegin(deviceId, { installId, expectedSha256, expectedSize });
await gateway.webapp.installChunk({ installId, offset, bytes, last }, { priority: 'bulk' });
const off = gateway.webapp.onWebappInstalled((id, info) => console.log('done', info));
```

Run `just codegen` from the repo root after touching a wire type. Never
hand-edit `dispatch.generated.ts`.
