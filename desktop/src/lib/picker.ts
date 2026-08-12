import { open, save } from '@tauri-apps/plugin-dialog';

export type ArtifactKind = 'daemon' | 'webapp';

const FILTERS: Record<ArtifactKind, { name: string; extensions: string[] }> = {
  daemon: { name: 'daemon or system image', extensions: ['swu', 'zst', 'bin', '*'] },
  webapp: { name: 'webapp bundle', extensions: ['zip'] },
};

export async function pickArtifact(kind: ArtifactKind): Promise<string | null> {
  const picked = await open({
    multiple: false,
    directory: false,
    title: kind === 'daemon' ? 'pick a daemon or image artifact' : 'pick a webapp bundle',
    filters: [FILTERS[kind]],
  });
  return typeof picked === 'string' ? picked : null;
}

export async function pickLogFile(defaultPath: string): Promise<string | null> {
  return await save({
    title: 'save the log lines',
    defaultPath,
    filters: [{ name: 'log', extensions: ['log', 'txt'] }],
  });
}
