import type { Method, Surface, SurfaceDocs, TypeDoc } from '../../sdk/types.ts';

function methodRefs(m: Method): string[] {
  const out: string[] = [];
  if (m.payload_ref) out.push(m.payload_ref);
  if (m.response) out.push(m.response);
  if (m.error) out.push(m.error);
  return out;
}

export function surfaceReferencedTypes(surface: Surface, all: Record<string, TypeDoc>): Array<[string, TypeDoc]> {
  const seen = new Set<string>();
  const order: string[] = [];
  const queue: string[] = [];

  for (const group of [surface.events, surface.requests, surface.commands, surface.handlers]) {
    for (const m of group) queue.push(...methodRefs(m));
  }

  while (queue.length) {
    const name = queue.shift()!;
    if (seen.has(name)) continue;
    const def = all[name];
    if (!def) continue;
    seen.add(name);
    order.push(name);
    if (def.kind === 'struct') {
      for (const f of def.fields) if (f.type_ref) queue.push(f.type_ref);
    } else {
      for (const v of def.variants) if (v.payload_ref) queue.push(v.payload_ref);
    }
  }

  return order.map(n => [n, all[n]!] as [string, TypeDoc]);
}

export function surfaceHasMethods(s: Surface): boolean {
  return s.events.length + s.requests.length + s.commands.length + s.handlers.length > 0;
}

export function methodCount(s: Surface): number {
  return s.events.length + s.requests.length + s.commands.length + s.handlers.length;
}

export type { Method, Surface, SurfaceDocs, TypeDoc };
