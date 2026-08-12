import type { GeoAccuracy, GeoError, Position } from '@bridgething/client';
import { BridgethingClient } from '@bridgething/client';

export type GeoShimConfig = { origin: string };

declare global {
  interface Window {
    __bridgethingGeo?: GeoShimConfig;
    __bridgethingGeoInstalled?: boolean;
  }
}

const WATCH_INTERVAL_MS = 1000;
const PERMISSION_DENIED = 1;
const POSITION_UNAVAILABLE = 2;
const TIMEOUT = 3;

function toCoords(p: Position): GeolocationPosition {
  return {
    coords: {
      latitude: p.lat,
      longitude: p.lon,
      altitude: p.altM ?? null,
      accuracy: p.accuracyM,
      altitudeAccuracy: null,
      heading: p.headingDeg ?? null,
      speed: p.speedMps ?? null,
      toJSON() {
        return { ...this };
      },
    },
    timestamp: p.tsUnixS * 1000,
    toJSON() {
      return { ...this };
    },
  } as GeolocationPosition;
}

function positionError(code: number, message: string): GeolocationPositionError {
  return { code, message, PERMISSION_DENIED, POSITION_UNAVAILABLE, TIMEOUT } as GeolocationPositionError;
}

function fromGeoError(error: GeoError): GeolocationPositionError {
  switch (error) {
    case 'permissionDenied':
      return positionError(PERMISSION_DENIED, 'location permission denied on the paired phone');
    case 'notDeclared':
      return positionError(PERMISSION_DENIED, 'this webapp does not declare the `geo` permission');
    default:
      return positionError(POSITION_UNAVAILABLE, 'no position available from the paired phone');
  }
}

function accuracyOf(options?: PositionOptions): GeoAccuracy {
  return options?.enableHighAccuracy ? 'fine' : 'coarse';
}

function maxAgeSecondsOf(options?: PositionOptions): number | null {
  const ms = options?.maximumAge;
  if (ms === undefined || !Number.isFinite(ms) || ms <= 0) return null;
  return Math.floor(ms / 1000);
}

class GeolocationBridge implements Geolocation {
  private readonly client: BridgethingClient;
  private nextWatchId = 1;
  private readonly watches = new Map<number, { token?: string; stop: () => void; released: boolean }>();

  constructor(cfg: GeoShimConfig) {
    const host = cfg.origin.replace(/^https?:\/\//, '');
    this.client = new BridgethingClient({ url: `ws://${host}/` });
  }

  getCurrentPosition(success: PositionCallback, error?: PositionErrorCallback | null, options?: PositionOptions): void {
    let settled = false;
    const finish = (fn: () => void) => {
      if (settled) return;
      settled = true;
      fn();
    };

    const timeoutMs = options?.timeout;
    const timer =
      timeoutMs !== undefined && Number.isFinite(timeoutMs)
        ? setTimeout(() => finish(() => error?.(positionError(TIMEOUT, 'timed out waiting for a fix'))), timeoutMs)
        : undefined;

    this.client.geo
      .getOnce({ accuracy: accuracyOf(options), maxAgeS: maxAgeSecondsOf(options) })
      .then(result => {
        if (timer !== undefined) clearTimeout(timer);
        finish(() => {
          if (result.ok) success(toCoords(result.response.position));
          else if (result.kind === 'domain') error?.(fromGeoError(result.error.error));
          else error?.(positionError(POSITION_UNAVAILABLE, 'the bridgething daemon refused the request'));
        });
      })
      .catch(() => {
        if (timer !== undefined) clearTimeout(timer);
        finish(() => error?.(positionError(POSITION_UNAVAILABLE, 'could not reach the bridgething daemon')));
      });
  }

  watchPosition(success: PositionCallback, error?: PositionErrorCallback | null, options?: PositionOptions): number {
    const id = this.nextWatchId++;
    const offPosition = this.client.geo.onPosition(p => success(toCoords(p)));
    const offError = this.client.geo.onErrorEvent(reply => error?.(fromGeoError(reply.error)));
    const entry = {
      token: undefined as string | undefined,
      released: false,
      stop: () => {
        offPosition();
        offError();
      },
    };
    this.watches.set(id, entry);

    this.client.geo
      .watch({ accuracy: accuracyOf(options), minIntervalMs: WATCH_INTERVAL_MS })
      .then(result => {
        if (!result.ok) {
          entry.stop();
          this.watches.delete(id);
          if (result.kind === 'domain') error?.(fromGeoError(result.error.error));
          else error?.(positionError(POSITION_UNAVAILABLE, 'the bridgething daemon refused the watch'));
          return;
        }
        if (entry.released) {
          void this.client.geo.unwatch({ token: result.response.token });
          return;
        }
        entry.token = result.response.token;
      })
      .catch(() => {
        entry.stop();
        this.watches.delete(id);
        error?.(positionError(POSITION_UNAVAILABLE, 'could not reach the bridgething daemon'));
      });

    return id;
  }

  clearWatch(id: number): void {
    const entry = this.watches.get(id);
    if (!entry) return;
    this.watches.delete(id);
    entry.released = true;
    entry.stop();
    if (entry.token !== undefined) void this.client.geo.unwatch({ token: entry.token });
  }
}

export function installGeolocationBridge(cfg: GeoShimConfig): void {
  const bridge = new GeolocationBridge(cfg);
  Object.defineProperty(navigator, 'geolocation', {
    value: bridge,
    configurable: true,
    enumerable: true,
  });
}

function boot(): void {
  const cfg = window.__bridgethingGeo;
  if (!cfg || window.__bridgethingGeoInstalled) return;
  window.__bridgethingGeoInstalled = true;
  installGeolocationBridge(cfg);
}

boot();
