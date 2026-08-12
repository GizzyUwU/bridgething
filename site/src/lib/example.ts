import type { Field, Method, TypeDoc } from '../../sdk/types.ts';

type Types = Record<string, TypeDoc>;
type Group = 'events' | 'requests' | 'commands' | 'handlers';

export type SigPart = { text: string } | { type: string } | { link: string; label: string; typeref?: string };

export const RESULT_ANCHOR = '/docs#request-results';

export function resultRefId(method: string): string {
  return `result-${method}`;
}

export function requestResultCode(m: Method): string {
  const response = m.response ?? 'void';
  const error = m.error ?? 'never';
  return `type Result =
  | { ok: true; response: ${response} }
  | { ok: false; kind: 'domain'; error: ${error} }
  | { ok: false; kind: 'protocol'; error: WireError };`;
}

function escHtml(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
}

const LINK_CLASS = 'text-accent border-b-0 hover:underline';

export function signatureHtml(m: Method, kind: Group, known: Set<string>): string {
  const name = `<a href="#m-${m.method}" class="text-fg hover:text-accent border-b-0 text-[0.95rem] font-semibold">${escHtml(m.method)}</a>`;
  const rest = signatureParts(m, kind)
    .map(part => {
      if ('text' in part) return escHtml(part.text);
      if ('link' in part) {
        const dt = part.typeref ? ` data-typeref="${part.typeref}"` : '';
        return `<a href="${part.link}"${dt} class="${LINK_CLASS}">${escHtml(part.label)}</a>`;
      }
      const type = escHtml(part.type);
      return known.has(part.type)
        ? `<a href="#type-${type}" data-typeref="${type}" class="${LINK_CLASS}">${type}</a>`
        : `<span class="text-fg/70">${type}</span>`;
    })
    .join('');
  return `${name}<span class="text-muted">${rest}</span>`;
}

export function signatureParts(m: Method, kind: Group): SigPart[] {
  const s = (text: string): SigPart => ({ text });
  const t = (type: string): SigPart => ({ type });
  const payload = (): SigPart[] => (m.payload ? [m.payload_ref ? t(m.payload_ref) : s(m.payload)] : []);

  if (kind === 'events') return [s('(handler: ('), ...payload(), s(') => void): () => void')];
  if (kind === 'commands') return [s('('), ...payload(), s('): Promise<void>')];
  if (kind === 'requests') {
    return [
      s('('),
      ...payload(),
      s('): Promise<'),
      { link: RESULT_ANCHOR, label: 'TypedRequestResult', typeref: resultRefId(m.method) },
      s('<'),
      m.response ? t(m.response) : s('void'),
      s(', '),
      m.error ? t(m.error) : s('never'),
      s('>>'),
    ];
  }
  return [s('(handle): '), m.response ? t(m.response) : s('void')];
}

function lowerFirst(s: string): string {
  return s ? s[0]!.toLowerCase() + s.slice(1) : s;
}

function paramName(typeName?: string): string {
  if (!typeName) return 'msg';
  for (const [suffix, name] of [
    ['Reply', 'reply'],
    ['Update', 'update'],
    ['Snapshot', 'snapshot'],
    ['State', 'state'],
    ['Event', 'event'],
    ['Entry', 'entry'],
  ] as const) {
    if (typeName.endsWith(suffix)) return name;
  }
  return lowerFirst(typeName);
}

function stringLiteral(fieldName: string): string {
  const n = fieldName.toLowerCase();
  if (n.includes('uri')) return `'spotify:track:...'`;
  if (n === 'mime') return `'image/jpeg'`;
  return `'...'`;
}

function valueFor(field: Field, types: Types): string {
  const ref = field.type_ref ? types[field.type_ref] : undefined;
  if (ref?.kind === 'enum') {
    const unit = ref.variants.every(v => !v.payload);
    if (unit && ref.variants[0]) return `'${lowerFirst(ref.variants[0].name)}'`;
    return `{ /* ${field.type_ref} */ }`;
  }
  if (field.type.endsWith('[]')) return '[]';
  if (field.type.startsWith('Record<')) return '{}';
  if (field.type === 'boolean') return 'true';
  if (field.type === 'number') return '0';
  if (field.type === 'string') return stringLiteral(field.name);
  if (ref?.kind === 'struct') return `{ /* ${field.type_ref} */ }`;
  return 'undefined';
}

function argLiteral(payloadRef: string | undefined, types: Types): string {
  if (!payloadRef) return '';
  const def = types[payloadRef];
  if (!def || def.kind !== 'struct' || def.fields.length === 0) return def ? '{}' : '';
  let chosen = def.fields.filter(f => !f.optional);
  if (chosen.length === 0) chosen = [def.fields[0]!];
  return `{ ${chosen.map(f => `${f.name}: ${valueFor(f, types)}`).join(', ')} }`;
}

function firstFieldAccess(typeName: string | undefined, types: Types, base: string): string | null {
  if (!typeName) return null;
  const def = types[typeName];
  if (def?.kind === 'struct' && def.fields[0]) return `${base}.${def.fields[0].name}`;
  return null;
}

export function methodExample(surfaceProp: string, m: Method, kind: Group, types: Types): string {
  const call = `client.${surfaceProp}.${m.method}`;

  if (kind === 'events') {
    const p = paramName(m.payload_ref ?? m.payload);
    const access = firstFieldAccess(m.payload_ref, types, p);
    const body = access ? `  console.log(${access});` : `  // ${p}: ${m.payload ?? 'void'}`;
    return `const off = ${call}((${p}) => {\n${body}\n});\n// call off() to unsubscribe`;
  }

  if (kind === 'commands') {
    return `await ${call}(${argLiteral(m.payload_ref, types)});`;
  }

  if (kind === 'requests') {
    const args = argLiteral(m.payload_ref, types);
    const access = firstFieldAccess(m.response, types, 'res.response');
    const okBody = access ? `  console.log(${access});` : `  // res.response: ${m.response ?? 'void'}`;
    const elseBody = m.error ? `\n} else {\n  console.warn(res.kind, res.error);` : '';
    return `const res = await ${call}(${args});\nif (res.ok) {\n${okBody}${elseBody}\n}`;
  }

  const arg = m.payload ? `, req` : '';
  return `${call}((handle${arg}) => {\n  handle.respond(/* ${m.response ?? '...'} */);\n});`;
}
