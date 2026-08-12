import matter from 'gray-matter';
import { readdir, readFile } from 'node:fs/promises';
import { basename, join } from 'node:path';
import type { ComponentNotes } from './changelog.ts';

export type ComponentReleaseFrontmatter = {
  version: string;
  channel: string;
  released_at: string;
  summary: string;
  yanked?: string | null;
  deprecated?: boolean;
  changelog_url?: string | null;
  min_image_version?: string | null;
  min_daemon_version?: string | null;
};

export type ComponentReleaseFile = ComponentReleaseFrontmatter &
  ComponentNotes & {
    basename: string;
    path: string;
  };

const REQUIRED_FRONTMATTER_FIELDS: (keyof ComponentReleaseFrontmatter)[] = [
  'version',
  'channel',
  'released_at',
  'summary',
];

export async function readComponentReleases(dir: string): Promise<ComponentReleaseFile[]> {
  let entries: string[];
  try {
    entries = await readdir(dir);
  } catch (err) {
    if ((err as NodeJS.ErrnoException).code === 'ENOENT') return [];
    throw err;
  }

  const files = entries.filter(e => e.endsWith('.md') && !e.startsWith('_'));
  const out: ComponentReleaseFile[] = [];

  for (const file of files) {
    const path = join(dir, file);
    const raw = await readFile(path, 'utf-8');
    const { data, content } = matter(raw);
    const fm = normalizeFrontmatter(data as Record<string, unknown>);

    for (const required of REQUIRED_FRONTMATTER_FIELDS) {
      if (typeof fm[required] !== 'string' || !fm[required]) {
        throw new Error(`${path}: frontmatter missing required string field "${required}"`);
      }
    }

    const fileBase = basename(file, '.md');
    if (fm['version'] !== fileBase) {
      throw new Error(
        `${path}: frontmatter version="${String(fm['version'])}" must equal filename basename "${fileBase}"`,
      );
    }

    out.push({
      basename: fileBase,
      path,
      version: fm['version'] as string,
      channel: fm['channel'] as string,
      released_at: fm['released_at'] as string,
      summary: fm['summary'] as string,
      yanked: typeof fm['yanked'] === 'string' ? fm['yanked'] : null,
      deprecated: fm['deprecated'] === true,
      changelog_url: typeof fm['changelog_url'] === 'string' ? fm['changelog_url'] : null,
      min_image_version: typeof fm['min_image_version'] === 'string' ? fm['min_image_version'] : null,
      min_daemon_version: typeof fm['min_daemon_version'] === 'string' ? fm['min_daemon_version'] : null,
      body: content,
    });
  }

  return out;
}

export function sortNewestFirst<T extends { released_at: string }>(items: T[]): T[] {
  return [...items].sort((a, b) => b.released_at.localeCompare(a.released_at));
}

function normalizeFrontmatter(fm: Record<string, unknown>): Record<string, unknown> {
  const out: Record<string, unknown> = {};
  for (const [key, value] of Object.entries(fm)) {
    if (value instanceof Date) {
      out[key] = value.toISOString().replace(/\.\d{3}Z$/, 'Z');
    } else {
      out[key] = value;
    }
  }
  return out;
}
