/**
 * A discoverer surfaces bridgething daemons reachable over the network
 * gateway WebSocket. The adapter consumes whatever the discoverer emits
 * and opens / closes connections per `Endpoint` lifecycle.
 *
 * Two bundled impls:
 *   - `StaticDiscoverer` — one fixed URL, works everywhere (browser too).
 *   - `MDNSDiscoverer` — browse `_bridgething._tcp` over multicast (Node /
 *     Bun only, optional `bonjour-service` peer dep).
 *
 * Callers can plug their own (e.g. a discoverer that polls a config file,
 * or one that resolves a list of known hostnames over plain DNS).
 */

/** A daemon reachable at `url`. `id` is stable for the lifetime of the
 *  endpoint and used as the adapter's `deviceId`. Re-announcing the same
 *  endpoint with the same id is a no-op. */
export type Endpoint = {
  id: string;
  url: string;
  name?: string;
  metadata?: Record<string, string>;
};

export type DiscoveryListener = { type: 'found'; endpoint: Endpoint } | { type: 'lost'; id: string };

export interface Discoverer {
  start(listener: (event: DiscoveryListener) => void): Promise<void>;
  stop(): Promise<void>;
}

/**
 * Single fixed endpoint. The common case: a developer plugged a Car Thing
 * in over USB-CDC-ECM and wants to hit `bridgething.local:8892`. Works in
 * the browser (no native deps) and on any modern host whose resolver
 * understands `.local` (most do via systemd-resolved / mDNSResponder /
 * Windows DNS Client).
 */
export class StaticDiscoverer implements Discoverer {
  private readonly endpoint: Endpoint;
  private started = false;

  constructor(options: { url: string; id?: string; name?: string } | string) {
    const opts = typeof options === 'string' ? { url: options } : options;
    this.endpoint = {
      id: opts.id ?? opts.url,
      url: opts.url,
      name: opts.name,
    };
  }

  start(listener: (event: DiscoveryListener) => void): Promise<void> {
    if (this.started) return Promise.resolve();
    this.started = true;
    listener({ type: 'found', endpoint: this.endpoint });
    return Promise.resolve();
  }

  stop(): Promise<void> {
    this.started = false;
    return Promise.resolve();
  }
}
