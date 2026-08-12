# @bridgething/browser

Drive a [bridgething](https://github.com/JoeyEamigh/bridgething) Spotify Car Thing
from a web page: install and switch webapps, push daemon and image updates, rename
the device.

```sh
bun add @bridgething/browser
```

## Connecting

```ts
import { Device } from '@bridgething/browser';

// over the usb-c cable, via the daemon's network gateway
const device = await Device.overNetwork('bridgething.local');

// or over an already-paired bluetooth peer, via Web Serial
const device = await Device.overSerial();
```

`overSerial()` resolves null when the user dismisses the chooser. Use
`serialAvailable()` to decide whether to offer it at all; iOS Safari and Firefox
have no Web Serial.

## Driving a device

```ts
const meta = await device.meta();
const webapps = await device.webapps();

await device.switchWebapp(webapps[0].id);
await device.installWebapp(
  new Uint8Array(await bundle.arrayBuffer()),
  catalogUrl,
);
await device.setNickname('the dashboard');
```

Updates are a push plus a feed:

```ts
const phase = await device.push('daemon', binary);
if (phase.kind === 'failed') console.error(phase.reason);
```

Manifest helpers - `fetchManifest`, `compositeVersion`, `otaArtifactUrls` - read
the same discover manifest the device does, so a page can work out what a device
should be running.
