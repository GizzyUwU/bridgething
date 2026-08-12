import { describe, expect, test } from 'bun:test';
import { artifactLabel, desktopRows, type DesktopPointer } from './desktop.ts';

function pointer(platforms: DesktopPointer['platforms']): DesktopPointer {
  return { version: '0.7.0', released_at: '2026-08-01T00:00:00Z', platforms };
}

function build(url: string) {
  return { url, size: 12 * 1024 * 1024, sha256: 'a'.repeat(64) };
}

describe('desktopRows', () => {
  test('lists every shipped target even with no pointer', () => {
    const rows = desktopRows(null);
    expect(rows.map(row => row.target)).toEqual([
      'darwin-aarch64',
      'darwin-x86_64',
      'linux-x86_64',
      'linux-aarch64',
      'windows-x86_64',
      'windows-aarch64',
    ]);
    expect(rows.every(row => row.build === null)).toBe(true);
  });

  test('labels a target the way a human names their machine', () => {
    const rows = desktopRows(null);
    expect(rows[0]).toMatchObject({ os: 'macos', arch: 'apple silicon', artifact: 'app bundle (.app.tar.gz)' });
    expect(rows[1]).toMatchObject({ os: 'macos', arch: 'intel' });
    expect(rows[3]).toMatchObject({ os: 'linux', arch: 'arm64', artifact: 'appimage' });
    expect(rows[5]).toMatchObject({ os: 'windows', arch: 'arm64', artifact: 'msi or exe installer' });
  });

  test('takes the artifact label from the published url', () => {
    const rows = desktopRows(
      pointer({
        'windows-aarch64': build('https://ota.bridgething.com/desktop/0.7.0/bridgething_0.7.0_arm64-setup.exe'),
      }),
    );
    expect(rows[5]?.artifact).toBe('exe installer');
    expect(rows[5]?.build?.size).toBe(12 * 1024 * 1024);
  });

  test('keeps a target the site does not know about yet', () => {
    const rows = desktopRows(
      pointer({ 'freebsd-x86_64': build('https://ota.bridgething.com/desktop/0.7.0/app.tar.gz') }),
    );
    expect(rows.at(-1)).toMatchObject({ target: 'freebsd-x86_64', os: 'freebsd', arch: 'x86_64' });
  });
});

describe('artifactLabel', () => {
  test('names each archive the release workflow can publish', () => {
    expect(artifactLabel('https://x/bridgething.app.tar.gz')).toBe('app bundle (.app.tar.gz)');
    expect(artifactLabel('https://x/bridgething_0.7.0_amd64.AppImage')).toBe('appimage');
    expect(artifactLabel('https://x/bridgething_0.7.0_x64_en-US.msi')).toBe('msi installer');
    expect(artifactLabel('https://x/bridgething_0.7.0_x64-setup.exe')).toBe('exe installer');
    expect(artifactLabel('https://x/bridgething.zip')).toBe('zip archive');
  });
});
