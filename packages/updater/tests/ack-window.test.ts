import { describe, expect, test } from 'bun:test';

import { AckRegistry, AckWindow } from '../src/ack-window';

describe('AckWindow', () => {
  test('resolves immediately when the offset is already within the window', async () => {
    const window = new AckWindow(0, { windowBytes: 1024 });
    expect(await window.waitForRoom(0)).toBe(true);
    expect(await window.waitForRoom(1023)).toBe(true);
  });

  test('blocks until note() moves the acked byte far enough to admit the offset', async () => {
    const window = new AckWindow(0, { windowBytes: 1024 });
    let resolved = false;
    const wait = window.waitForRoom(1024).then(ok => {
      resolved = true;
      return ok;
    });

    await new Promise(r => setTimeout(r, 20));
    expect(resolved).toBe(false);

    window.note(1);
    expect(await wait).toBe(true);
    expect(resolved).toBe(true);
  });

  test('seeding the baseline at construction admits a resumed offset with no acks yet', async () => {
    const window = new AckWindow(64 * 1024, { windowBytes: 32 * 1024 });
    expect(await window.waitForRoom(64 * 1024)).toBe(true);
  });

  test('returns false when no ack progress arrives before the timeout', async () => {
    const window = new AckWindow(0, { windowBytes: 1024, ackTimeoutMs: 30 });
    expect(await window.waitForRoom(1024)).toBe(false);
  });

  test('note() is a no-op for a stale (already-superseded) ack', async () => {
    const window = new AckWindow(0, { windowBytes: 1024 });
    window.note(500);
    window.note(100); // stale; must not move the baseline backwards
    expect(window.ackedBytes).toBe(500);
  });
});

describe('AckRegistry', () => {
  test('routes note() to the window registered for that transfer id', async () => {
    const registry = new AckRegistry({ windowBytes: 1024 });
    const a = registry.register('a', 0);
    const b = registry.register('b', 0);

    registry.note('a', 1024);
    expect(await a.waitForRoom(1024)).toBe(true);
    expect(b.ackedBytes).toBe(0);

    registry.deregister('a');
    registry.note('a', 9999); // must be silently dropped once deregistered
    expect(a.ackedBytes).toBe(1024);
  });
});
