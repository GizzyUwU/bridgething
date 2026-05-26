import { BRIDGETHING_MDNS_SERVICE_TYPE, BRIDGETHING_NETWORK_GATEWAY_PORT } from '@bridgething/lib';

import type { Discoverer, DiscoveryListener, Endpoint } from './discovery';

/**
 * Browse for `_bridgething._tcp` services and surface each as an `Endpoint`.
 * Node/Bun only; `bonjour-service` is loaded lazily so browser bundles are unaffected.
 *
 * The daemon advertises type=`_bridgething._tcp`, port=8892, and txt `nickname=<string>` when set.
 * The hostname comes from avahi's `%h.local` substitution (`bridgething.local` or
 * `bridgething-<short-serial>.local` on multi-device hosts).
 */

type BonjourLike = {
  find(opts: { type: string; protocol?: 'tcp' | 'udp' }, handler: (svc: BonjourService) => void): BonjourBrowser;
  destroy(): void;
};

type BonjourBrowser = {
  on(event: 'up' | 'down', handler: (svc: BonjourService) => void): void;
  stop(): void;
};

type BonjourService = {
  name: string;
  fqdn?: string;
  host: string;
  port: number;
  type: string;
  protocol: 'tcp' | 'udp';
  addresses?: string[];
  txt?: Record<string, string | Uint8Array>;
};

export type MDNSDiscovererOptions = {
  /** Service type without the `_…._tcp` decoration. Defaults to `bridgething`. */
  serviceType?: string;
  /** Override the URL builder. Default builds `ws://<host>:<port>/`. */
  buildUrl?: (service: { host: string; port: number; addresses: string[]; nickname?: string }) => string;
};

export class MDNSDiscoverer implements Discoverer {
  private readonly serviceType: string;
  private readonly buildUrl: NonNullable<MDNSDiscovererOptions['buildUrl']>;
  private bonjour: BonjourLike | null = null;
  private browser: BonjourBrowser | null = null;
  private readonly seen = new Map<string, Endpoint>();

  constructor(options: MDNSDiscovererOptions = {}) {
    this.serviceType = options.serviceType ?? BRIDGETHING_MDNS_SERVICE_TYPE;
    this.buildUrl = options.buildUrl ?? (({ host, port }) => `ws://${host}:${port}/`);
  }

  async start(listener: (event: DiscoveryListener) => void): Promise<void> {
    if (this.bonjour) return;
    const { Bonjour } = await loadBonjour();
    this.bonjour = new Bonjour() as BonjourLike;

    const handleUp = (svc: BonjourService) => {
      const endpoint = this.endpointFor(svc);
      if (!endpoint) return;
      const existing = this.seen.get(endpoint.id);
      if (existing && existing.url === endpoint.url) return;
      this.seen.set(endpoint.id, endpoint);
      listener({ type: 'found', endpoint });
    };

    const handleDown = (svc: BonjourService) => {
      const id = serviceId(svc);
      if (!this.seen.delete(id)) return;
      listener({ type: 'lost', id });
    };

    this.browser = this.bonjour.find({ type: this.serviceType, protocol: 'tcp' }, handleUp);
    this.browser.on('up', handleUp);
    this.browser.on('down', handleDown);
  }

  stop(): Promise<void> {
    this.browser?.stop();
    this.browser = null;
    this.bonjour?.destroy();
    this.bonjour = null;
    this.seen.clear();
    return Promise.resolve();
  }

  private endpointFor(svc: BonjourService): Endpoint | null {
    if (!svc.host || !svc.port) return null;
    const addresses = svc.addresses ?? [];
    const txt = normalizeTxt(svc.txt);
    const nickname = txt['nickname'];
    const url = this.buildUrl({
      host: svc.host,
      port: svc.port,
      addresses,
      nickname,
    });
    return {
      id: serviceId(svc),
      url,
      name: nickname || svc.name,
      metadata: txt,
    };
  }
}

/** Convenience factory mirroring the daemon's defaults. */
export function discoverBridgethingDaemons(options?: MDNSDiscovererOptions): MDNSDiscoverer {
  return new MDNSDiscoverer({
    serviceType: BRIDGETHING_MDNS_SERVICE_TYPE,
    buildUrl: ({ host, port }) => `ws://${host}:${port || BRIDGETHING_NETWORK_GATEWAY_PORT}/`,
    ...options,
  });
}

function serviceId(svc: BonjourService): string {
  return svc.fqdn ?? `${svc.name}.${svc.type}.${svc.protocol}`;
}

function normalizeTxt(txt: BonjourService['txt']): Record<string, string> {
  if (!txt) return {};
  const out: Record<string, string> = {};
  for (const [key, value] of Object.entries(txt)) {
    if (typeof value === 'string') {
      out[key] = value;
    } else if (value instanceof Uint8Array) {
      out[key] = new TextDecoder().decode(value);
    } else {
      out[key] = String(value);
    }
  }
  return out;
}

async function loadBonjour(): Promise<{ Bonjour: new () => unknown }> {
  try {
    const mod = (await import('bonjour-service')) as unknown as {
      Bonjour: new () => unknown;
      default?: { Bonjour: new () => unknown };
    };
    if (mod.Bonjour) return { Bonjour: mod.Bonjour };
    if (mod.default?.Bonjour) return { Bonjour: mod.default.Bonjour };
    throw new Error('bonjour-service module did not export Bonjour');
  } catch (err) {
    throw new Error(
      `@bridgething/adapter-network: MDNSDiscoverer needs the optional 'bonjour-service' dependency. ` +
        `Install it with 'bun add bonjour-service'. Underlying: ${err instanceof Error ? err.message : String(err)}`,
    );
  }
}
