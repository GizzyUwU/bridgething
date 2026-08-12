import { describe, expect, test } from 'bun:test';

import { describeError } from './errors.ts';

describe('describeError', () => {
  test('reads the reason out of a tagged command error', () => {
    expect(describeError({ kind: 'link', reason: 'the socket closed' })).toBe('the socket closed');
  });

  test('spaces a unit command error kind', () => {
    expect(describeError({ kind: 'notConnected' })).toBe('not connected');
  });

  test('keeps error messages and plain strings', () => {
    expect(describeError(new Error('boom'))).toBe('boom');
    expect(describeError('boom')).toBe('boom');
  });

  test('never renders object coercion', () => {
    expect(describeError({ status: 500 })).toBe('{"status":500}');
    expect(describeError({})).toBe('the reason was lost');
  });

  test('survives a rejection with nothing in it', () => {
    expect(describeError(undefined)).toBe('the reason was lost');
    expect(describeError(null)).toBe('the reason was lost');
  });
});
