import { describe, expect, test } from 'bun:test';
import { detectTarget } from './Download.tsx';

const MAC =
  'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Safari/605.1.15';
const WINDOWS =
  'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0.0.0 Safari/537.36';
const LINUX = 'Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0.0.0 Safari/537.36';
const IPHONE =
  'Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Mobile/15E148 Safari/604.1';

describe('detectTarget', () => {
  test('assumes apple silicon on a mac that claims to be intel', () => {
    expect(detectTarget(MAC)).toBe('darwin-aarch64');
  });

  test('lets the architecture hint overturn the guess', () => {
    expect(detectTarget(MAC, 'x86')).toBe('darwin-x86_64');
    expect(detectTarget(WINDOWS, 'arm')).toBe('windows-aarch64');
  });

  test('reads arm out of the user agent when there is no hint', () => {
    expect(detectTarget('Mozilla/5.0 (X11; Linux aarch64) AppleWebKit/537.36')).toBe('linux-aarch64');
    expect(detectTarget('Mozilla/5.0 (Windows NT 10.0; Win64; arm64) AppleWebKit/537.36')).toBe('windows-aarch64');
  });

  test('defaults to x86_64 elsewhere', () => {
    expect(detectTarget(WINDOWS)).toBe('windows-x86_64');
    expect(detectTarget(LINUX)).toBe('linux-x86_64');
  });

  test('detects nothing for a phone', () => {
    expect(detectTarget(IPHONE)).toBeNull();
    expect(detectTarget('Mozilla/5.0 (Linux; Android 15; Pixel 9) AppleWebKit/537.36')).toBeNull();
  });
});
