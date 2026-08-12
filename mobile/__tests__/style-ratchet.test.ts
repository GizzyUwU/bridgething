import { readdirSync, readFileSync, statSync } from 'fs';
import { join } from 'path';

const ROOT = join(__dirname, '..');
const ROOTS = ['screens', 'components', 'lib'];

const THEME = join('lib', 'theme.ts');
const ICON = join('components', 'Icon.tsx');

const FILES = ROOTS.flatMap(dir => walk(dir));

function walk(dir: string): string[] {
  const out: string[] = [];
  for (const entry of readdirSync(join(ROOT, dir))) {
    const rel = join(dir, entry);
    if (statSync(join(ROOT, rel)).isDirectory()) {
      out.push(...walk(rel));
    } else if (rel.endsWith('.ts') || rel.endsWith('.tsx')) {
      out.push(rel);
    }
  }
  return out;
}

function offenders(pattern: RegExp, exempt: string[] = []): string[] {
  const hits: string[] = [];
  for (const file of FILES) {
    if (exempt.includes(file)) continue;
    const lines = readFileSync(join(ROOT, file), 'utf8').split('\n');
    lines.forEach((line, index) => {
      if (pattern.test(line)) hits.push(`${file}:${index + 1}: ${line.trim()}`);
    });
  }
  return hits;
}

describe('style ratchet', () => {
  test('the sweep actually reads the app', () => {
    expect(FILES.length).toBeGreaterThan(40);
    expect(FILES).toContain(THEME);
    expect(FILES).toContain(ICON);
  });

  test('no native alert is left anywhere', () => {
    expect(offenders(/\bAlert\.alert\b/)).toEqual([]);
  });

  test('no literal colour lives outside the theme', () => {
    expect(
      offenders(
        /hsla?\(|rgba?\(|#[0-9a-fA-F]{6}\b|#[0-9a-fA-F]{3}\b/, //
        [THEME],
      ),
    ).toEqual([]);
  });

  test('lucide is reachable only through the icon wrapper', () => {
    expect(offenders(/lucide-react-native/, [ICON])).toEqual([]);
  });

  test('no depth is faked with shadows', () => {
    expect(
      offenders(
        /\bshadow(Color|Opacity|Radius|Offset)\b|\belevation\s*:|\bshadow-(sm|md|lg|xl|2xl|inner)\b/,
      ),
    ).toEqual([]);
  });

  test('scheme resolution stays in the theme layer', () => {
    expect(offenders(/\buseColorScheme\b/, [THEME])).toEqual([]);
  });
});
