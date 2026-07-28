import type { BridgethingOtaRun } from '@bridgething/session-react-native';

import { otaProgress } from '../lib/ota';
import { EPOCH, otaRun } from './fixtures';

function lifecycle(): BridgethingOtaRun[] {
  const frames: BridgethingOtaRun[] = [];
  for (let pct = 0; pct <= 100; pct += 10) {
    frames.push(
      otaRun({
        stepId: 0,
        phase: 'downloading',
        stageReceived: 1_000_000 * pct,
        stageTotal: 100_000_000,
      }),
    );
  }
  for (let pct = 0; pct <= 100; pct += 25) {
    frames.push(
      otaRun({
        stepId: 1,
        phase: 'writing',
        dwlPercent: pct,
        stageReceived: undefined,
        stageTotal: undefined,
      }),
    );
  }
  frames.push(otaRun({ stepId: 2, phase: 'reboot', phaseStartedAt: EPOCH }));
  return frames;
}

const percents = (runs: BridgethingOtaRun[], now = EPOCH) =>
  runs.map(run => otaProgress(run, now).percent);

describe('ota progress', () => {
  test('never runs backwards over the life of an update', () => {
    const seen = percents(lifecycle());
    const drops = seen.filter((p, i) => i > 0 && p < seen[i - 1]);

    expect(drops).toEqual([]);
  });

  test('stays within its own bounds', () => {
    for (const p of percents(lifecycle())) {
      expect(p).toBeGreaterThanOrEqual(0);
      expect(p).toBeLessThanOrEqual(100);
    }
  });

  test('does not move when only the transfer rate estimate changes', () => {
    const slow = otaRun({ ratePerSec: 1_000_000 });
    const fast = otaRun({ ratePerSec: 5_000_000 });

    expect(otaProgress(fast, EPOCH).percent).toBe(
      otaProgress(slow, EPOCH).percent,
    );
  });

  test('an unmeasurable rate does not move it either', () => {
    const measured = otaRun({ ratePerSec: 1_000_000 });
    const unknown = otaRun({ ratePerSec: undefined });

    expect(otaProgress(unknown, EPOCH).percent).toBe(
      otaProgress(measured, EPOCH).percent,
    );
  });

  test('the eta still gets shorter when the transfer speeds up', () => {
    const slow = otaProgress(otaRun({ ratePerSec: 1_000_000 }), EPOCH);
    const fast = otaProgress(otaRun({ ratePerSec: 8_000_000 }), EPOCH);

    expect(fast.etaSeconds).toBeLessThan(slow.etaSeconds ?? Infinity);
  });

  test('a finished update reads as finished', () => {
    const done = otaProgress(otaRun({ outcome: 'succeeded' }), EPOCH);

    expect(done.percent).toBe(100);
    expect(done.etaSeconds).toBe(0);
  });
});
