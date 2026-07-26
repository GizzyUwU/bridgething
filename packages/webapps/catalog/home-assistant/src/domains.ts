import type { HaState } from './ha';

export type ControlKind = 'toggle' | 'lock' | 'momentary' | 'climate' | 'readonly';

const TOGGLE_DOMAINS = new Set(['light', 'switch', 'input_boolean', 'fan', 'humidifier', 'siren']);
const MOMENTARY_DOMAINS = new Set(['scene', 'script', 'automation', 'button', 'input_button']);

// pre-checked in the picker on first run, capped downstream.
const DEFAULT_PICK_DOMAINS = new Set(['light', 'climate', 'scene']);

export function domainOf(entityId: string): string {
  const dot = entityId.indexOf('.');
  return dot < 0 ? entityId : entityId.slice(0, dot);
}

export function controlKind(entityId: string): ControlKind {
  const d = domainOf(entityId);
  if (d === 'climate') return 'climate';
  if (d === 'lock') return 'lock';
  if (d === 'cover') return 'toggle';
  if (TOGGLE_DOMAINS.has(d)) return 'toggle';
  if (MOMENTARY_DOMAINS.has(d)) return 'momentary';
  return 'readonly';
}

export function isControllable(entityId: string): boolean {
  return controlKind(entityId) !== 'readonly';
}

export function isDefaultPick(entityId: string): boolean {
  return DEFAULT_PICK_DOMAINS.has(domainOf(entityId));
}

export function isActive(s: HaState): boolean {
  const d = domainOf(s.entityId);
  if (d === 'cover') return s.state === 'open' || s.state === 'opening';
  if (d === 'lock') return s.state === 'unlocked';
  if (d === 'climate') return s.state !== 'off' && s.state !== 'unavailable';
  return s.state === 'on';
}

/** Service call that flips the entity, given its current state. */
export function toggleCall(s: HaState): { domain: string; service: string } {
  const d = domainOf(s.entityId);
  if (d === 'lock') return { domain: 'lock', service: s.state === 'locked' ? 'unlock' : 'lock' };
  if (d === 'cover') return { domain: 'cover', service: 'toggle' };
  return { domain: d, service: 'toggle' };
}

/** Service call for a momentary tile (scene/script/button). */
export function momentaryCall(entityId: string): { domain: string; service: string } {
  const d = domainOf(entityId);
  if (d === 'button' || d === 'input_button') return { domain: d, service: 'press' };
  if (d === 'automation') return { domain: 'automation', service: 'trigger' };
  return { domain: d, service: 'turn_on' };
}

/** Optimistic next state for a toggle/lock tile. */
export function optimisticToggle(s: HaState): string {
  const d = domainOf(s.entityId);
  if (d === 'cover') return s.state === 'open' ? 'closed' : 'open';
  if (d === 'lock') return s.state === 'locked' ? 'unlocked' : 'locked';
  return s.state === 'on' ? 'off' : 'on';
}

export function friendlyName(s: HaState): string {
  const fn = s.attributes['friendly_name'];
  if (typeof fn === 'string' && fn.length) return fn;
  const rest = s.entityId.slice(domainOf(s.entityId).length + 1);
  return rest.replace(/_/g, ' ');
}

export function num(attr: unknown): number | null {
  return typeof attr === 'number' && Number.isFinite(attr) ? attr : null;
}
