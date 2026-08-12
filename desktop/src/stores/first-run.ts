import { signal } from '@preact/signals';

const KEY = 'bridgething.first-run-done';

export const firstRunDone = signal(localStorage.getItem(KEY) === '1');

export function completeFirstRun(): void {
  if (firstRunDone.value) return;
  localStorage.setItem(KEY, '1');
  firstRunDone.value = true;
}
