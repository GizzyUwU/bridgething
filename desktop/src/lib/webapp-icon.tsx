import {
  RemoteIcon,
  fetchIcon,
  looksLikeSvg,
  useSession,
  type BoxSize,
  type DeviceSession,
  type ResolvedIcon,
} from '@bridgething/ui';
import type { VNode } from 'preact';

export function WebappIcon({
  id,
  iconHash,
  name,
  size = 'md',
  class: className,
}: {
  id: string;
  iconHash: string | null;
  name: string;
  size?: BoxSize;
  class?: string;
}): VNode {
  const session = useSession();

  return (
    <RemoteIcon cacheKey={iconHash} source={() => resolveIcon(session, id)} name={name} size={size} class={className} />
  );
}

export function CatalogIcon({
  url,
  name,
  size = 'md',
  class: className,
}: {
  url: string | null;
  name: string;
  size?: BoxSize;
  class?: string;
}): VNode {
  return (
    <RemoteIcon
      cacheKey={url}
      source={signal => fetchIcon(url ?? '', signal)}
      name={name}
      size={size}
      class={className}
    />
  );
}

async function resolveIcon(session: DeviceSession, id: string): Promise<ResolvedIcon> {
  const resource = await session.webappResource(id, 'icon');
  const bytes = Uint8Array.from(resource.bytes);
  const text = new TextDecoder().decode(bytes);
  if (looksLikeSvg(resource.mime, text)) return { kind: 'svg', svg: text };
  return { kind: 'raster', url: `data:${resource.mime ?? 'image/png'};base64,${base64(bytes)}` };
}

function base64(bytes: Uint8Array): string {
  let raw = '';
  for (const byte of bytes) raw += String.fromCharCode(byte);
  return btoa(raw);
}
