import { expect, test } from 'bun:test';
import { BundleStore, DeliveryClient, Discovery, initLogging } from '../index.js';

test('the addon loads and exposes the delivery surface', () => {
  expect(typeof DeliveryClient.connect).toBe('function');
  expect(typeof initLogging).toBe('function');
  expect(typeof BundleStore).toBe('function');
  expect(typeof Discovery).toBe('function');
});

test('a fresh browser has found nothing yet and hands back a snapshot', () => {
  const discovery = new Discovery();
  expect(discovery.endpoints()).toEqual([]);
});

test('a connect to nothing rejects rather than hanging', async () => {
  await expect(DeliveryClient.connect('ws://127.0.0.1:1/')).rejects.toThrow();
});

test('an unknown bundle kind is refused at construction', () => {
  expect(() => new BundleStore('sqlite', 'android', '/tmp/bridgething-core-node-test')).toThrow();
});

test('an unknown bundle platform is refused at construction', () => {
  expect(() => new BundleStore('nlu', 'symbian', '/tmp/bridgething-core-node-test')).toThrow();
});

test('a store with nothing downloaded reports absent and holds no path', () => {
  const store = new BundleStore('nlu', 'android', '/tmp/bridgething-core-node-test-empty');
  const status = store.status();
  expect(status.state).toBe('absent');
  expect(status.path).toBeUndefined();
});
