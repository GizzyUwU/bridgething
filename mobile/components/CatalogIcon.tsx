import { useEffect, useState } from 'react';
import { Image, Text, View } from 'react-native';
import { SvgXml } from 'react-native-svg';

import { boundedCache } from '../lib/bounded-cache';

type Resolved =
  | { kind: 'svg'; xml: string }
  | { kind: 'raster' }
  | { kind: 'failed' };

const ICON_MAX_BYTES = 64 * 1024;
const ICON_FETCH_TIMEOUT_MS = 10_000;
const ICON_CACHE_LIMIT = 96;
const MAX_INFLIGHT_FETCHES = 4;

const cache = boundedCache<Resolved>(ICON_CACHE_LIMIT);

let inflight = 0;
const waiting: (() => void)[] = [];

async function acquireFetchSlot(): Promise<void> {
  if (inflight < MAX_INFLIGHT_FETCHES) {
    inflight += 1;
    return;
  }
  await new Promise<void>(resolve => waiting.push(resolve));
  inflight += 1;
}

function releaseFetchSlot(): void {
  inflight -= 1;
  waiting.shift()?.();
}

function looksLikeSvg(contentType: string | null, body: string): boolean {
  if (contentType && /(^|[/+])svg/i.test(contentType)) return true;
  return /^\s*(<\?xml[^>]*\?>\s*)?(<!--.*?-->\s*)*<svg[\s/>]/is.test(
    body.slice(0, 512),
  );
}

export function CatalogIcon({
  url,
  name,
  size,
  radiusClass = 'rounded-xl',
}: {
  url: string | null;
  name: string;
  size: number;
  radiusClass?: string;
}) {
  const [resolved, setResolved] = useState<Resolved | null>(null);

  useEffect(() => {
    if (!url) {
      setResolved(null);
      return;
    }
    const cached = cache.get(url);
    if (cached) {
      setResolved(cached);
      return;
    }
    setResolved(null);
    let cancelled = false;
    const abort = new AbortController();
    let timer: ReturnType<typeof setTimeout> | null = null;
    (async () => {
      await acquireFetchSlot();
      if (cancelled) {
        releaseFetchSlot();
        return;
      }
      timer = setTimeout(() => abort.abort(), ICON_FETCH_TIMEOUT_MS);
      try {
        const res = await fetch(url, { signal: abort.signal });
        if (!res.ok) throw new Error(`icon fetch failed (${res.status})`);
        const declared = Number(res.headers.get('content-length'));
        if (Number.isFinite(declared) && declared > ICON_MAX_BYTES) {
          throw new Error('icon larger than 64 KiB');
        }
        const type = res.headers.get('content-type');
        const undecided = !type || /octet-stream|^text\/plain/i.test(type);
        const svgByHeader = !!type && /(^|[/+])svg/i.test(type);
        let next: Resolved = { kind: 'raster' };
        if (svgByHeader || undecided) {
          const body = await res.text();
          if (body.length > ICON_MAX_BYTES) {
            throw new Error('icon larger than 64 KiB');
          }
          next = looksLikeSvg(type, body)
            ? { kind: 'svg', xml: body }
            : { kind: 'raster' };
        }
        cache.set(url, next);
        if (!cancelled) setResolved(next);
      } catch {
        if (cancelled) return;
        cache.set(url, { kind: 'failed' });
        setResolved({ kind: 'failed' });
      } finally {
        if (timer) clearTimeout(timer);
        releaseFetchSlot();
      }
    })();
    return () => {
      cancelled = true;
      abort.abort();
    };
  }, [url]);

  const dims = { width: size, height: size };

  return (
    <View
      className={`items-center justify-center overflow-hidden bg-secondary ${radiusClass}`}
      style={dims}
    >
      {resolved?.kind === 'svg' ? (
        <SvgXml xml={resolved.xml} width={size} height={size} />
      ) : resolved?.kind === 'raster' && url ? (
        <Image
          source={{ uri: url }}
          style={dims}
          resizeMode="cover"
          onError={() => {
            cache.set(url, { kind: 'failed' });
            setResolved({ kind: 'failed' });
          }}
        />
      ) : url && resolved === null ? null : (
        <Text className="text-[16px] font-extrabold text-foreground">
          {name.charAt(0).toUpperCase()}
        </Text>
      )}
    </View>
  );
}
