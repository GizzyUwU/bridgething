import type { CatalogAppListing } from '@bridgething/catalog';

import { PALETTE, toneColor, toneSoftColor, type Tone } from '../lib/theme';
import {
  listingState,
  logLevelTone,
  TONE_BG,
  TONE_BORDER,
  TONE_DOT,
  TONE_TEXT,
} from '../lib/tone';

const TONES: Tone[] = [
  'neutral',
  'accent',
  'ok',
  'err',
  'warn',
  'experimental',
];

function listing(over: Partial<CatalogAppListing>): CatalogAppListing {
  return {
    app: {
      id: 'com.example.app',
      name: 'example',
      description: 'an app',
      icon: null,
      versions: [],
    },
    sourceUrl: 'https://example.invalid/catalog.json',
    alsoAvailableFrom: [],
    newestCompatible: null,
    installedVersion: null,
    updateAvailable: false,
    ...over,
  } as CatalogAppListing;
}

const compatible = { version: '1.0.0' } as NonNullable<
  CatalogAppListing['newestCompatible']
>;

describe('tone tables', () => {
  test.each(TONES)('%s has an entry in every table', tone => {
    expect(TONE_BG[tone]).toBeTruthy();
    expect(TONE_BORDER[tone]).toBeTruthy();
    expect(TONE_DOT[tone]).toBeTruthy();
    expect(TONE_TEXT[tone]).toBeTruthy();
  });

  test('the tables carry no tone the taxonomy does not name', () => {
    for (const table of [TONE_BG, TONE_BORDER, TONE_DOT, TONE_TEXT]) {
      expect(Object.keys(table).sort()).toEqual([...TONES].sort());
    }
  });

  test.each(TONES)('%s resolves to a palette colour in both schemes', tone => {
    for (const scheme of ['light', 'dark'] as const) {
      const palette = PALETTE[scheme];
      const values = Object.values(palette);
      expect(values).toContain(toneColor(palette, tone));
      expect(values).toContain(toneSoftColor(palette, tone));
    }
  });

  test('neutral is the only tone that resolves to a non-hued colour', () => {
    const palette = PALETTE.dark;
    expect(toneColor(palette, 'neutral')).toBe(palette.soft);
    expect(toneSoftColor(palette, 'neutral')).toBe(palette.neutralSoft);
    expect(toneColor(palette, 'accent')).toBe(palette.accent);
    expect(toneSoftColor(palette, 'err')).toBe(palette.errSoft);
  });
});

describe('logLevelTone', () => {
  test('severity reads as tone', () => {
    expect(logLevelTone('error')).toBe('err');
    expect(logLevelTone('warn')).toBe('warn');
    expect(logLevelTone('info')).toBe('accent');
    expect(logLevelTone('debug')).toBe('neutral');
    expect(logLevelTone('trace')).toBe('neutral');
  });

  test('a level the daemon has not taught us stays neutral', () => {
    expect(logLevelTone('notice')).toBe('neutral');
    expect(logLevelTone('')).toBe('neutral');
  });

  test('every mapped level resolves in the tone tables', () => {
    for (const level of ['trace', 'debug', 'info', 'warn', 'error', 'other']) {
      const tone = logLevelTone(level);
      expect(TONE_BG[tone]).toBeTruthy();
      expect(TONE_TEXT[tone]).toBeTruthy();
    }
  });
});

describe('listingState', () => {
  test('an app with no compatible version reads as incompatible', () => {
    expect(listingState(listing({ newestCompatible: null }))).toEqual({
      label: 'incompatible',
      tone: 'neutral',
    });
  });

  test('an update outranks the installed state', () => {
    expect(
      listingState(
        listing({
          newestCompatible: compatible,
          installedVersion: '0.9.0',
          updateAvailable: true,
        }),
      ),
    ).toEqual({ label: 'update', tone: 'accent' });
  });

  test('an installed app with no update reads as installed', () => {
    expect(
      listingState(
        listing({ newestCompatible: compatible, installedVersion: '1.0.0' }),
      ),
    ).toEqual({ label: 'installed', tone: 'ok' });
  });

  test('an uninstalled compatible app offers install', () => {
    expect(listingState(listing({ newestCompatible: compatible }))).toEqual({
      label: 'install',
      tone: 'neutral',
    });
  });
});
