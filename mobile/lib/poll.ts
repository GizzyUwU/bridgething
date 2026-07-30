import { useIsFocused } from '@react-navigation/native';
import { useEffect, useRef } from 'react';

import { useAppActive } from './app-active';

export function useAppActiveInterval(
  fn: () => void,
  intervalMs: number,
  enabled = true,
): void {
  const appActive = useAppActive();
  const live = enabled && appActive;

  const latest = useRef(fn);
  latest.current = fn;

  useEffect(() => {
    if (!live) return;
    const tick = () => latest.current();
    tick();
    const id = setInterval(tick, intervalMs);
    return () => clearInterval(id);
  }, [live, intervalMs]);
}

export function usePoll(fn: () => void, intervalMs: number): void {
  useAppActiveInterval(fn, intervalMs, useIsFocused());
}
