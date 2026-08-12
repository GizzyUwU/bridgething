import type { TypeDoc } from '../../sdk/types.ts';

function lowerFirst(s: string): string {
  return s ? s[0]!.toLowerCase() + s.slice(1) : s;
}

function oneLine(s: string): string {
  return s.replace(/\s+/g, ' ').trim();
}

function comment(desc?: string): string {
  return desc ? ` // ${oneLine(desc)}` : '';
}

export function renderTypeTs(name: string, def: TypeDoc): string {
  if (def.kind === 'struct') {
    if (def.fields.length === 0) return `type ${name} = {};`;
    const lines = def.fields.map(f => `  ${f.name}${f.optional ? '?' : ''}: ${f.type};${comment(f.description)}`);
    return `type ${name} = {\n${lines.join('\n')}\n};`;
  }

  const unit = def.variants.every(v => !v.payload);
  if (unit) {
    const anyDesc = def.variants.some(v => v.description);
    const members = def.variants.map(v => `'${lowerFirst(v.name)}'`);
    if (!anyDesc && members.join(' | ').length <= 60) {
      return `type ${name} = ${members.join(' | ')};`;
    }
    const lines = def.variants.map(v => `  | '${lowerFirst(v.name)}'${comment(v.description)}`);
    return `type ${name} =\n${lines.join('\n')};`;
  }

  const tag = def.tag ?? 'type';
  const content = def.content ?? 'data';
  const lines = def.variants.map(v => {
    const disc = `${tag}: '${lowerFirst(v.name)}'`;
    const shape = v.payload ? `{ ${disc}; ${content}: ${v.payload} }` : `{ ${disc} }`;
    return `  | ${shape}${comment(v.description)}`;
  });
  return `type ${name} =\n${lines.join('\n')};`;
}
