import { BridgethingGateway } from '@bridgething/gateway';
import type { BridgeToGatewayMsg, OtaPhase } from '@bridgething/lib/gateway';
import { newUuid } from '@bridgething/lib/uuid';
import { describe, expect, test } from 'bun:test';

import { OtaDriver } from '../src/driver';
import { bytesArtifactSource } from '../src/source';
import { FakeDaemon } from './fake-daemon';

const DEVICE_ID = 'fake-device';

function otaBeginAck(requestId: string, resumeFromOffset: number): BridgeToGatewayMsg {
  return {
    id: newUuid(),
    meta: { kind: 'response', data: { requestId } },
    data: { type: 'system', data: { event: 'otaBeginAck', data: { resumeFromOffset } } },
  };
}

function transferAck(transferId: string, received: number): BridgeToGatewayMsg {
  return {
    id: newUuid(),
    meta: { kind: 'event' },
    data: { type: 'transfer', data: { event: 'ack', data: { transferId, received } } },
  };
}

function otaProgress(phase: OtaPhase, percent: number): BridgeToGatewayMsg {
  return {
    id: newUuid(),
    meta: { kind: 'event' },
    data: { type: 'system', data: { event: 'otaProgress', data: { phase, percent, etaMs: null } } },
  };
}

function otaError(msg: string): BridgeToGatewayMsg {
  return {
    id: newUuid(),
    meta: { kind: 'event' },
    data: { type: 'system', data: { event: 'otaError', data: { code: 'offsetMismatch', msg } } },
  };
}

/** Drains fragments for `transferId` one at a time, acking each as it arrives, until `total` bytes
 * have been sent. Mirrors `OtaStreamTests.swift`'s `drainFragments`. */
async function drainFragments(daemon: FakeDaemon, transferId: string, total: number): Promise<void> {
  let sent = 0;
  while (sent < total) {
    const msg = await daemon.waitForNext(
      m => m.data.type === 'transfer' && m.data.data.event === 'fragment' && m.data.data.data.transferId === transferId,
    );
    if (msg.data.type !== 'transfer' || msg.data.data.event !== 'fragment') throw new Error('unreachable');
    const fragment = msg.data.data.data;
    sent = fragment.offset + fragment.bytes.length;
    daemon.sendToDriver(DEVICE_ID, transferAck(transferId, sent));
  }
}

async function nextBegin(daemon: FakeDaemon): Promise<{ requestId: string; transferId: string; totalSize: number }> {
  const msg = await daemon.waitForNext(
    m => m.meta.kind === 'request' && m.data.type === 'system' && m.data.data.event === 'otaBegin',
  );
  if (msg.data.type !== 'system' || msg.data.data.event !== 'otaBegin') throw new Error('unreachable');
  return {
    requestId: msg.id,
    transferId: msg.data.data.data.transfer.id,
    totalSize: msg.data.data.data.transfer.totalSize,
  };
}

function boot(): { daemon: FakeDaemon; driver: OtaDriver } {
  const daemon = new FakeDaemon();
  const gateway = new BridgethingGateway(daemon);
  void gateway.start();
  daemon.connect(DEVICE_ID);
  const driver = new OtaDriver(gateway, DEVICE_ID);
  return { daemon, driver };
}

describe('OtaDriver', () => {
  test('pushDaemon stays within the ack window and completes on activate + reboot', async () => {
    const { daemon, driver } = boot();
    // window/fragment defaults are 512 KiB / 64 KiB - 600 KiB guarantees a fragment lands exactly
    // on the window boundary (offset 512 KiB), which must not arrive until acked.
    const payload = new Uint8Array(600 * 1024).map((_, i) => i % 251);
    const source = bytesArtifactSource(payload);

    const pushDone = driver.pushDaemon(source);
    const begin = await nextBegin(daemon);
    expect(begin.totalSize).toBe(payload.byteLength);
    daemon.sendToDriver(DEVICE_ID, otaBeginAck(begin.requestId, 0));

    let received = 0;
    for (let i = 0; i < 8; i++) {
      const msg = await daemon.waitForNext(
        m =>
          m.data.type === 'transfer' &&
          m.data.data.event === 'fragment' &&
          m.data.data.data.transferId === begin.transferId,
      );
      if (msg.data.type !== 'transfer' || msg.data.data.event !== 'fragment') throw new Error('unreachable');
      const f = msg.data.data.data;
      expect(f.offset).toBe(i * 64 * 1024);
      received = f.offset + f.bytes.length;
    }
    // the 9th fragment (offset 512 KiB) is exactly at the window boundary and must not arrive yet.
    await expect(
      daemon.waitForNext(
        m =>
          m.data.type === 'transfer' &&
          m.data.data.event === 'fragment' &&
          m.data.data.data.transferId === begin.transferId,
        200,
      ),
    ).rejects.toThrow();

    daemon.sendToDriver(DEVICE_ID, transferAck(begin.transferId, received));
    await drainFragments(daemon, begin.transferId, payload.byteLength);

    daemon.sendToDriver(DEVICE_ID, otaProgress('writing', 100));
    await daemon.waitForNext(m => m.data.type === 'system' && m.data.data.event === 'otaActivate');
    daemon.sendToDriver(DEVICE_ID, otaProgress('reboot', 100));

    expect(await pushDone).toEqual({ phase: 'completed' });
    driver.close();
  });

  test('an OtaError mid-stream cancels the stream and abandons the transfer', async () => {
    const { daemon, driver } = boot();
    const payload = new Uint8Array(600 * 1024);
    const source = bytesArtifactSource(payload);

    const pushDone = driver.pushDaemon(source);
    const begin = await nextBegin(daemon);
    daemon.sendToDriver(DEVICE_ID, otaBeginAck(begin.requestId, 0));

    // let a couple fragments flow, acked, so the stream is mid-flight rather than window-blocked.
    for (let i = 0; i < 2; i++) {
      const msg = await daemon.waitForNext(
        m =>
          m.data.type === 'transfer' &&
          m.data.data.event === 'fragment' &&
          m.data.data.data.transferId === begin.transferId,
      );
      if (msg.data.type !== 'transfer' || msg.data.data.event !== 'fragment') throw new Error('unreachable');
      const f = msg.data.data.data;
      daemon.sendToDriver(DEVICE_ID, transferAck(begin.transferId, f.offset + f.bytes.length));
    }

    daemon.sendToDriver(DEVICE_ID, otaError('synthetic'));

    const abandon = await daemon.waitForNext(
      m =>
        m.data.type === 'transfer' &&
        m.data.data.event === 'abandon' &&
        m.data.data.data.transferId === begin.transferId,
    );
    if (abandon.data.type !== 'transfer' || abandon.data.data.event !== 'abandon') throw new Error('unreachable');
    expect(abandon.data.data.data.transferId).toBe(begin.transferId);

    const result = await pushDone;
    expect(result.phase).toBe('failed');
    driver.close();
  });

  test('resuming from a non-zero offset seeds the baseline and streams the remainder', async () => {
    const { daemon, driver } = boot();
    const payloadSize = 160 * 1024;
    const resumeOffset = 64 * 1024;
    const payload = new Uint8Array(payloadSize).map((_, i) => i % 251);
    const source = bytesArtifactSource(payload);

    const pushDone = driver.pushDaemon(source);
    const begin = await nextBegin(daemon);
    // the daemon already holds resumeOffset bytes and reports it as the resume point.
    daemon.sendToDriver(DEVICE_ID, otaBeginAck(begin.requestId, resumeOffset));

    // regression coverage for the deadlock this ports from host-gateway/companion: without seeding
    // the ack-window baseline to resumeOffset, the first resume fragment never sends at all.
    const first = await daemon.waitForNext(
      m =>
        m.data.type === 'transfer' &&
        m.data.data.event === 'fragment' &&
        m.data.data.data.transferId === begin.transferId,
    );
    if (first.data.type !== 'transfer' || first.data.data.event !== 'fragment') throw new Error('unreachable');
    expect(first.data.data.data.offset).toBe(resumeOffset);
    daemon.sendToDriver(
      DEVICE_ID,
      transferAck(begin.transferId, first.data.data.data.offset + first.data.data.data.bytes.length),
    );

    await drainFragments(daemon, begin.transferId, payloadSize);

    daemon.sendToDriver(DEVICE_ID, otaProgress('writing', 100));
    await daemon.waitForNext(m => m.data.type === 'system' && m.data.data.event === 'otaActivate');
    daemon.sendToDriver(DEVICE_ID, otaProgress('reboot', 100));

    expect(await pushDone).toEqual({ phase: 'completed' });
    driver.close();
  });

  test('a transfer with no acks ever resolves to a failed (stalled) snapshot', async () => {
    const daemon = new FakeDaemon();
    const gateway = new BridgethingGateway(daemon);
    void gateway.start();
    daemon.connect(DEVICE_ID);
    // small window + short ack-timeout so the stall path fires fast instead of the real 30s default.
    // payload must exceed one fragment (64 KiB) so the *second* fragment is gated on the window and
    // genuinely stalls, rather than the whole (small) artifact fitting in the first ungated read.
    const driver = new OtaDriver(gateway, DEVICE_ID, { windowBytes: 1024, ackTimeoutMs: 30 });
    const payload = new Uint8Array(200 * 1024);
    const source = bytesArtifactSource(payload);

    const pushDone = driver.pushDaemon(source);
    const begin = await nextBegin(daemon);
    daemon.sendToDriver(DEVICE_ID, otaBeginAck(begin.requestId, 0));

    const result = await pushDone;
    expect(result.phase).toBe('failed');
    if (result.phase === 'failed') expect(result.reason).toContain('stalled');
    driver.close();
  });
});
