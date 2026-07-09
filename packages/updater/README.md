# @bridgething/updater

Transport-agnostic OTA updater for a [bridgething](https://github.com/JoeyEamigh/bridgething)
Spotify Car Thing: a `bunx`/`npx` CLI that brings a device to the latest release
on a channel, plus the library the CLI is built from.

It updates the **device** (daemon binary and/or full image), not a webapp. To
install or share a webapp, use [`@bridgething/client`](https://www.npmjs.com/package/@bridgething/client)
and the `push`/`share` scripts a [`create-bridgething`](https://www.npmjs.com/package/create-bridgething)
project ships.

## CLI

```sh
bunx @bridgething/updater
```

Connects to a Car Thing over the daemon's network gateway (the USB-gadget link
by default), reads the discover manifest, resolves the target channel's `latest`
composite version, downloads whatever artifacts are missing, and pushes the
update. If the image half of the version changed it pushes a full image OTA,
otherwise it pushes just the daemon binary. A `latest` release that is yanked or
deprecated is refused.

Options:

| Flag | Default | Meaning |
| --- | --- | --- |
| `--root <url>` | `https://ota.bridgething.com` | Manifest + artifact root. |
| `--channel <name>` | `stable` | Channel to track. |
| `--host <ws-url>` | `ws://bridgething.local:8892/` | Daemon network gateway. |
| `--cache-dir <path>` | OS tmpdir `/bridgething-updater` | Artifact download cache. |
| `--daemon-only` | off | Skip the image OTA even if the image half changed. |

Multiple devices on the network resolve to distinct mDNS names; point `--host` at
the one you mean:

```sh
bunx @bridgething/updater --host ws://bridgething-<serial>.local:8892/
```

## Library

The CLI is a thin driver over `OtaDriver`, which streams an artifact to the
daemon with sender-side flow control and reports progress to a terminal state.

```ts
import { BridgethingGateway } from '@bridgething/gateway';
import { NetworkAdapter } from '@bridgething/adapter-network';
import { OtaDriver } from '@bridgething/updater';
import { fileArtifactSource } from '@bridgething/updater/node';

const gateway = new BridgethingGateway(new NetworkAdapter());
await gateway.start();
// resolve a deviceId from a 'version' announce event, then:

const driver = new OtaDriver(gateway, deviceId);
try {
  const snapshot = await driver.pushDaemon(await fileArtifactSource('./bridgething'), s =>
    console.log(s.phase, 'percent' in s ? s.percent : ''),
  );
  if (snapshot.phase === 'failed') console.error(snapshot.reason);
} finally {
  driver.close();
}
```

`OtaDriver` covers every OTA kind the daemon accepts:

- `pushImage({ source, zcks, updateUrlBase })` - full image OTA. When the daemon
  requests delta ranges of the compressed asset, `zcks` (a map of asset name to
  source) serves them back over the same link; `serveAssetRanges` wires that up.
- `pushDaemon(source)` - stage and activate a new daemon binary.
- `pushBuiltinWebapp(source)` - stage and activate a built-in webapp.
- `pushBandaidBatch(artifacts)` - stage several bandaid artifacts, then commit
  them together.
- `installWebapp(source)` - install a third-party webapp bundle.

### Artifact sources

An `ArtifactSource` is a random-access, sized byte source. The driver reads it in
64 KiB fragments and never buffers the whole file. Construct one from wherever
the bytes live:

- `fileArtifactSource(path)` from `@bridgething/updater/node` - a filesystem file.
- `bytesArtifactSource(bytes)` - an in-memory `Uint8Array`.
- `blobArtifactSource(blob)` - a browser `Blob` / `File`.

### Manifest helpers

`@bridgething/updater/manifest` exposes the discover-manifest types and helpers
the CLI uses: `fetchManifest`, `parseCompositeVersion`, `otaArtifactUrls`, and
`builtinWebappUrl`.

### Adapters

Re-exported for convenience so a consumer needs one import:

- `@bridgething/updater/websocket` - `NetworkAdapter` (network gateway, any host
  that can open a WebSocket).
- `@bridgething/updater/web-serial` - `WebSerialAdapter` (in-browser Bluetooth
  Classic over Web Serial).

## Posture

The network gateway has no auth, matching the project posture. Treat the update
path like a debug interface: run it over the USB-CDC-ECM gadget link or a trusted
LAN, not an exposed one.
