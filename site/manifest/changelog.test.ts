import { describe, expect, test } from 'bun:test';
import { compose, composeVersion, parseVersion } from './changelog.ts';

const daemon = {
  version: '0.1.0',
  summary: 'ANCS notifications and time sync.',
  body: '## Highlights\n\n- ANCS GATT client.\n- Time sync.\n',
};

const image = {
  version: '2026.01.0',
  summary: 'Initial image release.',
  body: '## Highlights\n\n- Mainline kernel.\n',
};

describe('compose()', () => {
  test('daemon-only bump renders image as no-change', () => {
    const out = compose({
      daemon,
      image,
      daemonBumped: true,
      imageBumped: false,
    });
    expect(out.summary).toBe(daemon.summary);
    expect(out.changelog).toContain('## daemon 0.1.0');
    expect(out.changelog).toContain('## image 2026.01.0');
    expect(out.changelog).toContain('ANCS GATT client');
    expect(out.changelog).toContain('_no change since previous release._');
    expect(out.changelog).not.toContain('Mainline kernel');
  });

  test('image-only bump renders daemon as no-change and uses image summary', () => {
    const out = compose({
      daemon,
      image,
      daemonBumped: false,
      imageBumped: true,
    });
    expect(out.summary).toBe(image.summary);
    expect(out.changelog).toContain('Mainline kernel');
    expect(out.changelog).toContain('_no change since previous release._');
    expect(out.changelog).not.toContain('ANCS GATT client');
  });

  test('both-bump renders both bodies and prefers daemon summary', () => {
    const out = compose({
      daemon,
      image,
      daemonBumped: true,
      imageBumped: true,
    });
    expect(out.summary).toBe(daemon.summary);
    expect(out.changelog).toContain('ANCS GATT client');
    expect(out.changelog).toContain('Mainline kernel');
    expect(out.changelog).not.toContain('_no change since previous release._');
  });

  test('neither-bump throws', () => {
    expect(() => compose({ daemon, image, daemonBumped: false, imageBumped: false })).toThrow(
      /neither component bumped/,
    );
  });
});

describe('version round-trip', () => {
  test('compose then parse returns the inputs', () => {
    const v = composeVersion('0.8.4', '2026.05.0');
    expect(v).toBe('0.8.4+image.2026.05.0');
    expect(parseVersion(v)).toEqual({ daemon: '0.8.4', image: '2026.05.0' });
  });

  test('parse rejects malformed', () => {
    expect(() => parseVersion('0.8.4')).toThrow();
    expect(() => parseVersion('0.8.4+2026.05.0')).toThrow();
    expect(() => parseVersion('0.8.4+image.')).toThrow();
  });
});
