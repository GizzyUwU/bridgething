/**
 * Minimal `@bridgething/gateway` demo wired up against the network
 * adapter (the dev-tether path: USB-CDC-ECM gadget + `bridgething.local`).
 *
 * Run with:
 *   bun run dev               # static URL, bridgething.local:8892
 *   BRIDGETHING_DISCOVERY=mdns bun run dev:mdns   # mDNS browse
 *
 * Connects, announces a fake `GatewayCapabilities` so the daemon knows
 * a peer is here, prints the daemon's `BridgeThingMeta`, queries the
 * installed webapp list, then idles. Ctrl-C to exit.
 */

import { NetworkAdapter } from '@bridgething/adapter-network';
import { MDNSDiscoverer } from '@bridgething/adapter-network/mdns';
import { BridgethingGateway, type GatewayEvent } from '@bridgething/gateway';
import { BRIDGETHING_NETWORK_GATEWAY_URL, LIB_VERSION, LIBBRIDGETHING_VERSION, LogVerbosity } from '@bridgething/lib';
import type { GatewayCapabilities } from '@bridgething/lib/shared';

const useMdns = process.env['BRIDGETHING_DISCOVERY'] === 'mdns';
const url = process.env['BRIDGETHING_URL'] ?? BRIDGETHING_NETWORK_GATEWAY_URL;

const adapter = useMdns
  ? new NetworkAdapter({ discovery: new MDNSDiscoverer(), logLevel: LogVerbosity.Log })
  : new NetworkAdapter({ discovery: url, logLevel: LogVerbosity.Log });

const gateway = new BridgethingGateway(adapter, { logLevel: LogVerbosity.Log });

const handleEvent = (event: GatewayEvent) => {
  switch (event.type) {
    case 'connected':
      console.log('++ connected:', event.device);
      onConnected(event.device.id).catch(err => console.error('post-connect work failed:', err));
      break;
    case 'disconnected':
      console.log('-- disconnected:', event.deviceId);
      break;
    case 'message':
      // Domain-level events are surfaced via the typed surfaces below;
      // the raw stream is only useful for diagnostics.
      break;
    case 'decodeError':
      console.error('!! decode error on', event.deviceId, event.description);
      break;
  }
};

gateway.version.on((deviceId, meta) => {
  console.log(`>> [${deviceId}] BridgeThingMeta:`, {
    nickname: meta.nickname,
    appVersion: meta.appVersion,
    imageVariant: meta.imageVariant,
    imageVersion: meta.imageVersion,
    serialNumber: meta.serialNumber,
  });
});

gateway.system.onDeviceNicknameChanged((deviceId, msg) => {
  console.log(`>> [${deviceId}] nickname changed:`, msg.nickname);
});

async function onConnected(deviceId: string): Promise<void> {
  const announce: GatewayCapabilities = {
    gateway: {
      address: '00:00:00:00:00:00',
      name: 'node-dev',
      osName: process.platform,
      appName: 'node-dev',
      appVersion: '0.1.0',
      adapterVersion: 'network',
      libVersion: LIB_VERSION,
      libbridgethingVersion: LIBBRIDGETHING_VERSION,
    },
    uriSchemes: [],
    network: { kind: 'ethernet', metered: false },
    available: {
      geo: false,
      notifications: false,
      netFetch: false,
      netWs: false,
      audioTts: false,
      lyrics: false,
    },
    audio: { earcons: [], voices: [] },
    musicProvider: 'none',
  };
  await gateway.send(deviceId, {
    id: crypto.randomUUID(),
    meta: { kind: 'event' },
    data: { type: 'capabilities', data: { event: 'announce', data: announce } },
  });

  const listed = await gateway.webapp.list(deviceId);
  if (listed.ok) {
    console.log(`>> [${deviceId}] installed webapps:`);
    for (const w of listed.response.webapps) {
      console.log(`     - ${w.name} (${w.id}) v${w.version} [${w.source}/${w.role}]`);
    }
  } else {
    console.warn(`>> [${deviceId}] webapp.list failed:`, listed);
  }
}

gateway.on(handleEvent);

await gateway.start();
console.log(useMdns ? 'browsing mDNS for _bridgething._tcp ...' : `dialing ${url} ...`);

const shutdown = async () => {
  console.log('\n>> shutting down ...');
  try {
    await gateway.stop();
  } finally {
    process.exit(0);
  }
};

process.on('SIGINT', shutdown);
process.on('SIGTERM', shutdown);

// Hold the event loop open.
await new Promise(() => {});
