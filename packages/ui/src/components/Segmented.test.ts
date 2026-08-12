import { describe, expect, test } from 'bun:test';

import { segments } from './Segmented.tsx';

describe('segments', () => {
  test('bare strings become options labelled by their own value', () => {
    expect(segments(['stable', 'beta'], 'stable')).toEqual([
      { value: 'stable', label: 'stable', selected: true },
      { value: 'beta', label: 'beta', selected: false },
    ]);
  });

  test('labelled options keep their label and select on value', () => {
    expect(
      segments(
        [
          { value: 'stable', label: 'Stable' },
          { value: 'beta', label: 'Beta' },
        ],
        'beta',
      ),
    ).toEqual([
      { value: 'stable', label: 'Stable', selected: false },
      { value: 'beta', label: 'Beta', selected: true },
    ]);
  });

  test('exactly one option is selected even when values repeat', () => {
    const selected = segments(['a', 'b', 'c'], 'b').filter(segment => segment.selected);
    expect(selected).toHaveLength(1);
    expect(selected[0]?.value).toBe('b');
  });

  test('a value outside the options selects nothing rather than falling back to the first', () => {
    expect(segments(['a', 'b'], 'c' as 'a' | 'b').some(segment => segment.selected)).toBe(false);
  });

  test('source order is preserved', () => {
    expect(segments(['c', 'a', 'b'], 'a').map(segment => segment.value)).toEqual(['c', 'a', 'b']);
  });

  test('no options yields no segments', () => {
    expect(segments([] as string[], 'a')).toEqual([]);
  });
});
