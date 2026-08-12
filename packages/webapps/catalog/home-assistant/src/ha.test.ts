import { describe, expect, test } from 'bun:test';
import { applyEntitiesEvent, type HaEntities } from './ha';

function seed(): HaEntities {
  const base = applyEntitiesEvent(
    {},
    {
      a: {
        'light.kitchen': { s: 'off', a: { friendly_name: 'Kitchen' } },
        'sensor.temp': { s: '20', a: { unit_of_measurement: 'C' } },
        'lock.front': { s: 'locked', a: {} },
      },
    },
  );
  if (!base) throw new Error('seed produced no snapshot');
  return base;
}

describe('applyEntitiesEvent', () => {
  test('adds entities from an `a` event', () => {
    const next = seed();
    expect(Object.keys(next).sort()).toEqual(['light.kitchen', 'lock.front', 'sensor.temp']);
    expect(next['sensor.temp']).toEqual({
      entityId: 'sensor.temp',
      state: '20',
      attributes: { unit_of_measurement: 'C' },
    });
  });

  test('a state diff replaces only the changed entity, leaving the others referentially identical', () => {
    const prev = seed();
    const next = applyEntitiesEvent(prev, { c: { 'sensor.temp': { '+': { s: '21' } } } });

    expect(next).not.toBeNull();
    expect(next).not.toBe(prev);
    expect(next!['sensor.temp']).not.toBe(prev['sensor.temp']);
    expect(next!['sensor.temp'].state).toBe('21');
    expect(next!['light.kitchen']).toBe(prev['light.kitchen']);
    expect(next!['lock.front']).toBe(prev['lock.front']);
  });

  test('never mutates the snapshot it was handed', () => {
    const prev = seed();
    const before = prev['sensor.temp'];
    applyEntitiesEvent(prev, { c: { 'sensor.temp': { '+': { s: '21', a: { extra: 1 } } } } });

    expect(prev['sensor.temp']).toBe(before);
    expect(before.state).toBe('20');
    expect(before.attributes).toEqual({ unit_of_measurement: 'C' });
  });

  test('an attribute-only diff replaces the attributes object and keeps the state string', () => {
    const prev = seed();
    const next = applyEntitiesEvent(prev, { c: { 'sensor.temp': { '+': { a: { battery: 90 } } } } })!;

    expect(next['sensor.temp'].state).toBe('20');
    expect(next['sensor.temp'].attributes).toEqual({ unit_of_measurement: 'C', battery: 90 });
    expect(next['sensor.temp'].attributes).not.toBe(prev['sensor.temp'].attributes);
  });

  test('a removed attribute drops off the new entity without touching the old one', () => {
    const prev = seed();
    const next = applyEntitiesEvent(prev, { c: { 'sensor.temp': { '-': { a: { unit_of_measurement: null } } } } })!;

    expect(next['sensor.temp'].attributes).toEqual({});
    expect(prev['sensor.temp'].attributes).toEqual({ unit_of_measurement: 'C' });
  });

  test('a diff that changes nothing reports no update at all', () => {
    const prev = seed();
    expect(applyEntitiesEvent(prev, { c: { 'sensor.temp': { '+': { s: '20' } } } })).toBeNull();
    expect(applyEntitiesEvent(prev, {})).toBeNull();
    expect(applyEntitiesEvent(prev, { c: { 'light.unknown': { '+': { s: 'on' } } } })).toBeNull();
    expect(applyEntitiesEvent(prev, { r: ['light.unknown'] })).toBeNull();
  });

  test('an `r` event drops the entity and leaves the rest referentially identical', () => {
    const prev = seed();
    const next = applyEntitiesEvent(prev, { r: ['lock.front'] })!;

    expect(next['lock.front']).toBeUndefined();
    expect(prev['lock.front']).toBeDefined();
    expect(next['sensor.temp']).toBe(prev['sensor.temp']);
  });
});
