import init, { FlashThing, type GestureReason } from 'flashthing-wasm';
import wasmBinary from 'flashthing-wasm/flashthing_wasm_bg.wasm?url';
import { ZipReader } from './zip-reader';

const BUNDLE_CACHE = 'bridgething-bundles-v1';

export type FlashEvent =
  | { type: 'findingDevice' }
  | { type: 'connecting' }
  | { type: 'connected' }
  | { type: 'bl2Boot' }
  | { type: 'resetting' }
  | { type: 'deviceMode'; mode: 'normal' | 'usb' | 'usbBurn' | 'notFound' }
  | { type: 'step'; step: number; data: { type: string } }
  | { type: 'flashProgress'; data: FlashProgress };

export type FlashProgress = {
  percent: number;
  elapsed: number;
  eta: number;
  rate: number;
  avgChunkTime: number;
  avgRate: number;
};

export type BundleSource = 'cache' | 'network';

let ready: Promise<unknown> | null = null;

function loadWasm(): Promise<unknown> {
  ready ??= init({ module_or_path: wasmBinary });
  return ready;
}

export function webusbSupported(): boolean {
  return typeof navigator !== 'undefined' && 'usb' in navigator;
}

function cacheStore(): Promise<Cache> | null {
  return typeof caches === 'undefined' ? null : caches.open(BUNDLE_CACHE);
}

async function cachedBundle(url: string, expectedSize: number): Promise<Blob | null> {
  const store = await cacheStore()?.catch(() => null);
  if (!store) return null;

  const hit = await store.match(url).catch(() => undefined);
  if (!hit) return null;

  const blob = await hit.blob();
  if (expectedSize && blob.size !== expectedSize) {
    await store.delete(url).catch(() => {});
    return null;
  }
  return blob;
}

async function storeBundle(url: string, blob: Blob): Promise<boolean> {
  const store = await cacheStore()?.catch(() => null);
  if (!store) return false;
  try {
    await store.put(url, new Response(blob));
    return true;
  } catch {
    return false;
  }
}

export type BundleResult = { blob: Blob; source: BundleSource; cached: boolean };

export async function loadBundle(
  url: string,
  expectedSize: number,
  onProgress: (received: number, total: number) => void,
): Promise<BundleResult> {
  const hit = await cachedBundle(url, expectedSize);
  if (hit) return { blob: hit, source: 'cache', cached: true };

  const res = await fetch(url);
  if (!res.ok) throw new Error(`fetch bundle: ${res.status}`);

  const total = Number(res.headers.get('content-length') ?? expectedSize ?? 0);
  let blob: Blob;

  if (res.body) {
    const reader = res.body.getReader();
    const chunks: BlobPart[] = [];
    let received = 0;
    for (;;) {
      const { done, value } = await reader.read();
      if (done) break;
      chunks.push(value);
      received += value.length;
      onProgress(received, total);
    }
    blob = new Blob(chunks, { type: 'application/zip' });
  } else {
    blob = await res.blob();
  }

  const cached = await storeBundle(url, blob);
  return { blob, source: 'network', cached };
}

export type FlashHandle = {
  steps: number;
  run(): Promise<void>;
};

export async function prepareFlash(
  bundle: Blob,
  onEvent: (event: FlashEvent) => void,
  awaitGesture: (reason: GestureReason) => Promise<void>,
): Promise<FlashHandle> {
  if (!webusbSupported()) throw new Error('this browser has no webusb. use chrome, edge, or another chromium browser.');

  const [, zip] = await Promise.all([loadWasm(), ZipReader.open(bundle)]);

  const meta = new TextDecoder().decode(await zip.readAll('meta.json'));

  const flasher = new FlashThing(onEvent as (event: unknown) => void, {
    readAll: (path: string) => zip.readAll(path),
    open: async (path: string) => {
      const { size, reader } = await zip.open(path);
      return { size, read: (n: number) => reader.read(n) };
    },
    awaitGesture,
    logLevelDirective: 'info',
  });

  await flasher.openJson(meta);

  return {
    steps: flasher.getNumSteps(),
    async run() {
      try {
        await flasher.flash();
      } finally {
        flasher.free();
      }
    },
  };
}
