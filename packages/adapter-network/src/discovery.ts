/**
 * A discoverer surfaces bridgething daemons reachable over the network gateway WebSocket.
 * The adapter opens/closes connections as `Endpoint` lifecycle events arrive.
 *
 * Two bundled implementations:
 *   - `StaticDiscoverer`: one fixed URL, works everywhere including browsers.
 *   - `MDNSDiscoverer`: browses `_bridgething._tcp` (Node/Bun only; optional `bonjour-service` peer dep).
 *
 * Callers can supply their own (e.g. polling a config file, resolving hostnames over DNS).
 */

/** A daemon reachable at `url`. `id` is stable for the endpoint's lifetime and used as the adapter's `deviceId`. */
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
 * Single fixed endpoint. The common case: USB-CDC-ECM tether with `bridgething.local:8892`.
 * Works in the browser and on any host whose resolver understands `.local`.
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
