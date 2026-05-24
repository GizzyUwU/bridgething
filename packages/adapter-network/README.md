# @bridgething/adapter-network

Network-gateway transport for [`@bridgething/gateway`](../gateway/typescript).
Speaks the daemon's WebSocket gateway on TCP port 8892 (`bridgething.local`
by default), so any host that can open a WebSocket can drive a Car Thing
without native code, NAPI bindings, or platform-specific Bluetooth stacks.

Replaces the retired `@bridgething/adapter-node`.

## Quick start

```ts
import { BridgethingGateway } from '@bridgething/gateway';
import { NetworkAdapter } from '@bridgething/adapter-network';

const adapter = new NetworkAdapter(); // defaults to ws://bridgething.local:8892/
const gateway = new BridgethingGateway(adapter);

gateway.on(event => {
  if (event.type === 'connected') console.log('up:', event.device);
  if (event.type === 'disconnected') console.log('down:', event.deviceId);
});

await gateway.start();
```

## Single device, custom host

```ts
new NetworkAdapter({ discovery: 'ws://10.42.1.1:8892/' });
new NetworkAdapter({ discovery: { id: 'lab-1', url: 'ws://thing-a.local:8892/', name: 'Lab A' } });
```

## Multiple known hosts

```ts
new NetworkAdapter({
  discovery: [
    { id: 'rack-1', url: 'ws://thing-a.local:8892/' },
    { id: 'rack-2', url: 'ws://thing-b.local:8892/' },
  ],
});
```

## mDNS browsing

For Node / Bun. Install the optional peer dependency first:

```sh
bun add bonjour-service
```

```ts
import { NetworkAdapter } from '@bridgething/adapter-network';
import { MDNSDiscoverer } from '@bridgething/adapter-network/mdns';

const adapter = new NetworkAdapter({ discovery: new MDNSDiscoverer() });
```

`MDNSDiscoverer` browses `_bridgething._tcp` and surfaces each daemon as
its own `Endpoint` with the device's mDNS-advertised nickname propagated
through `Endpoint.metadata.nickname`.

## Browser support

`NetworkAdapter` works in browsers that can open WebSockets. The default
URL (`ws://bridgething.local:8892/`) relies on the OS resolver for `.local`
hostnames (mDNS via systemd-resolved / mDNSResponder / Windows DNS Client).
mDNS *browsing* is not available in browsers; use the static-URL form.

For the in-browser Bluetooth Classic path, use `WebSerialAdapter` from
`@bridgething/gateway` instead — `NetworkAdapter` covers the dev-tether
(USB-CDC-ECM) case only.

## Reconnection

Per-peer auto-reconnect with exponential backoff (`500ms → 15s`, ladder
repeats). Configure with `options.reconnect`:

```ts
new NetworkAdapter({ reconnect: false });                  // off
new NetworkAdapter({ reconnect: [1000, 5000, 30000] });    // custom ladder
```

## Posture

The network gateway has no auth (matches the project posture). Treat the
WebSocket like a debug interface: only expose it on the USB-CDC-ECM
gadget link or a trusted LAN. The daemon binds `0.0.0.0:8892`, so deny
external reach with the host firewall if the device is wifi-bridged.
