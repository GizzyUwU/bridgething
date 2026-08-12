export const FEATURES = {
  browserFlasher: process.env['BRIDGETHING_BROWSER_FLASHER'] === '0',
  terbium: process.env['BRIDGETHING_TERBIUM'] !== '1',
} as const;
