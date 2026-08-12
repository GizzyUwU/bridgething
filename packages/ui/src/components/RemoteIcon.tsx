import type { VNode } from 'preact';
import { useEffect, useRef, useState } from 'preact/hooks';

import { cx } from '../cx.ts';
import { cacheIcon, cachedIcon, svgDataUrl, type ResolvedIcon } from '../icon.ts';
import { BOX, BOX_TEXT, type BoxSize } from '../tokens.ts';

export function RemoteIcon({
  cacheKey,
  source,
  name,
  size = 'md',
  class: className,
}: {
  cacheKey: string | null;
  source: (signal: AbortSignal) => Promise<ResolvedIcon>;
  name: string;
  size?: BoxSize;
  class?: string;
}): VNode {
  const [icon, setIcon] = useState<ResolvedIcon | null>(() => (cacheKey ? (cachedIcon(cacheKey) ?? null) : null));

  const held = useRef(source);
  held.current = source;

  useEffect(() => {
    if (!cacheKey) {
      setIcon(null);
      return;
    }

    const hit = cachedIcon(cacheKey);
    if (hit) {
      setIcon(hit);
      return;
    }

    setIcon(null);
    let cancelled = false;
    const abort = new AbortController();

    held
      .current(abort.signal)
      .then(next => {
        if (cancelled) return;
        cacheIcon(cacheKey, next);
        setIcon(next);
      })
      .catch(() => {
        if (cancelled) return;
        setIcon({ kind: 'failed' });
      });

    return () => {
      cancelled = true;
      abort.abort();
    };
  }, [cacheKey]);

  const src = icon?.kind === 'svg' ? svgDataUrl(icon.svg) : icon?.kind === 'raster' ? icon.url : null;
  const pending = cacheKey !== null && icon === null;

  return (
    <span
      class={cx(
        'inline-flex shrink-0 items-center justify-center overflow-hidden bg-neutral-soft',
        BOX[size],
        className,
      )}>
      {src ? (
        <img
          src={src}
          alt=""
          class="size-full object-cover"
          onError={() => {
            if (cacheKey) cacheIcon(cacheKey, { kind: 'failed' });
            setIcon({ kind: 'failed' });
          }}
        />
      ) : pending ? null : (
        <span aria-hidden="true" class={cx('font-display font-medium text-soft', BOX_TEXT[size])}>
          {name.slice(0, 1).toUpperCase()}
        </span>
      )}
    </span>
  );
}
