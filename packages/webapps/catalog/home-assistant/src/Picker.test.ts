import { describe, expect, test } from 'bun:test';
import { PICKER_COLS, PICKER_ROW_PITCH, visibleRange } from './Picker';

const VIEWPORT = 376;

function onScreen(count: number, scrollTop: number, viewportHeight: number): number[] {
  const hit: number[] = [];
  for (let i = 0; i < count; i++) {
    const top = Math.floor(i / PICKER_COLS) * PICKER_ROW_PITCH;
    if (top + PICKER_ROW_PITCH > scrollTop && top < scrollTop + viewportHeight) hit.push(i);
  }
  return hit;
}

describe('visibleRange', () => {
  test('an empty list renders nothing', () => {
    expect(visibleRange(0, 0, VIEWPORT)).toEqual({ start: 0, end: 0 });
  });

  test('a list shorter than the viewport renders whole', () => {
    expect(visibleRange(6, 0, VIEWPORT)).toEqual({ start: 0, end: 6 });
  });

  test('a long list renders a small fraction of it', () => {
    const { start, end } = visibleRange(500, 0, VIEWPORT);
    expect(start).toBe(0);
    expect(end).toBeLessThan(40);
  });

  test('every item actually on screen is inside the range, at any scroll offset', () => {
    const count = 501;
    const maxScroll = Math.ceil(count / PICKER_COLS) * PICKER_ROW_PITCH;
    for (let scrollTop = 0; scrollTop <= maxScroll; scrollTop += 7) {
      const { start, end } = visibleRange(count, scrollTop, VIEWPORT);
      for (const i of onScreen(count, scrollTop, VIEWPORT)) {
        expect(i).toBeGreaterThanOrEqual(start);
        expect(i).toBeLessThan(end);
      }
    }
  });

  test('the range never runs past the end of the list', () => {
    const count = 37;
    const maxScroll = Math.ceil(count / PICKER_COLS) * PICKER_ROW_PITCH;
    for (let scrollTop = 0; scrollTop <= maxScroll + 500; scrollTop += 11) {
      const { start, end } = visibleRange(count, scrollTop, VIEWPORT);
      expect(end).toBeLessThanOrEqual(count);
      expect(start).toBeLessThanOrEqual(end);
    }
  });

  test('the range starts on a row boundary so the two columns stay aligned', () => {
    for (let scrollTop = 0; scrollTop < 4000; scrollTop += 13) {
      expect(visibleRange(500, scrollTop, VIEWPORT).start % PICKER_COLS).toBe(0);
    }
  });

  test('scrolling deep into the list drops the rows above it', () => {
    const { start } = visibleRange(500, 100 * PICKER_ROW_PITCH, VIEWPORT);
    expect(start).toBeGreaterThan(150);
  });

  test('an unmeasured viewport still renders the top of the list', () => {
    const { start, end } = visibleRange(500, 0, 0);
    expect(start).toBe(0);
    expect(end).toBeGreaterThan(0);
  });
});
