import { useEffect, useState } from 'react';
import { Image, Text, View } from 'react-native';
import { SvgXml } from 'react-native-svg';

type Resolved =
  | { kind: 'svg'; xml: string }
  | { kind: 'raster' }
  | { kind: 'failed' };

const cache = new Map<string, Resolved>();

const ICON_MAX_BYTES = 64 * 1024;
const ICON_FETCH_TIMEOUT_MS = 10_000;

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
    const timer = setTimeout(() => abort.abort(), ICON_FETCH_TIMEOUT_MS);
    (async () => {
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
        cache.set(url, { kind: 'failed' });
        if (!cancelled) setResolved({ kind: 'failed' });
      } finally {
        clearTimeout(timer);
      }
    })();
    return () => {
      cancelled = true;
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
