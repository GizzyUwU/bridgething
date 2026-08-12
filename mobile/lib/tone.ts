import type { CatalogAppListing } from '@bridgething/catalog';
import { TONE_DOT, type Tone } from '@bridgething/ui/tokens';

export { TONE_DOT };

export const TONE_BG: Record<Tone, string> = {
  neutral: 'bg-neutral-soft',
  accent: 'bg-accent-soft',
  ok: 'bg-ok-soft',
  err: 'bg-err-soft',
  warn: 'bg-warn-soft',
  experimental: 'bg-experimental-soft',
};

export const TONE_BORDER: Record<Tone, string> = {
  neutral: 'border-rule',
  accent: 'border-accent',
  ok: 'border-ok',
  err: 'border-err',
  warn: 'border-warn',
  experimental: 'border-experimental',
};

export const TONE_TEXT: Record<Tone, string> = {
  neutral: 'text-soft',
  accent: 'text-accent',
  ok: 'text-ok',
  err: 'text-err',
  warn: 'text-warn',
  experimental: 'text-experimental',
};

const LOG_LEVEL_TONE: Record<string, Tone> = {
  trace: 'neutral',
  debug: 'neutral',
  info: 'accent',
  warn: 'warn',
  error: 'err',
};

export function logLevelTone(level: string): Tone {
  return LOG_LEVEL_TONE[level] ?? 'neutral';
}

export type ListingState = { label: string; tone: Tone };

export function listingState(listing: CatalogAppListing): ListingState {
  if (!listing.newestCompatible)
    return { label: 'incompatible', tone: 'neutral' };
  if (listing.updateAvailable) return { label: 'update', tone: 'accent' };
  if (listing.installedVersion) return { label: 'installed', tone: 'ok' };
  return { label: 'install', tone: 'neutral' };
}
