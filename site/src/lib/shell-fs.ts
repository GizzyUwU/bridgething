import { Bash, defineCommand, getCommandNames } from 'just-bash/browser';

export type ShellFsManifest = {
  image: string;
  dirs: string[];
  links: Array<[path: string, target: string]>;
  files: Array<[path: string, size: number, mode: number, content: number | string]>;
  texts: string[];
};

export type BlobLoader = (hash: string) => Promise<string>;

function formatSize(n: number): string {
  if (n < 1024) return `${n} bytes`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KiB`;
  return `${(n / (1024 * 1024)).toFixed(1)} MiB`;
}

function placeholder(content: number, size: number): string {
  if (content === -2) {
    return `ELF aarch64, ${formatSize(size)}, stripped\nbinary contents are not part of the web build. flash the image for the real thing.\n`;
  }
  if (content === -3) {
    return `text, ${formatSize(size)}. not included in the web build.\n`;
  }
  return `binary, ${formatSize(size)}. contents are not part of the web build.\n`;
}

function imageMtime(image: string): Date | undefined {
  const m = image.match(/(\d{4})(\d{2})(\d{2})(\d{2})(\d{2})(\d{2})/);
  if (!m) return undefined;
  return new Date(Date.UTC(+m[1]!, +m[2]! - 1, +m[3]!, +m[4]!, +m[5]!, +m[6]!));
}

const uname = defineCommand('uname', async args => ({
  stdout: args.length > 0 ? 'Linux bridgething 7.0.2-bridgething #1 SMP PREEMPT aarch64 GNU/Linux\n' : 'Linux\n',
  stderr: '',
  exitCode: 0,
}));

function fetchBlob(hash: string): Promise<string> {
  return fetch(`/shell-fs/${hash}.txt`).then(res => {
    if (!res.ok) throw new Error(`blob fetch failed (${res.status})`);
    return res.text();
  });
}

export async function createImageShell(manifest: ShellFsManifest, loadBlob: BlobLoader = fetchBlob): Promise<Bash> {
  const mtime = imageMtime(manifest.image);
  const real = new Map(manifest.files.map(f => [f[0], f]));
  const files: Record<string, { content: string; mode: number; mtime?: Date } | (() => Promise<string>)> = {};
  const lazyMeta: Array<[path: string, mode: number]> = [];

  function resolveContent(content: number | string, size: number): string | (() => Promise<string>) {
    if (typeof content === 'string') return () => loadBlob(content);
    return content >= 0 ? manifest.texts[content]! : placeholder(content, size);
  }

  for (const [path, size, mode, content] of manifest.files) {
    const resolved = resolveContent(content, size);
    if (typeof resolved === 'function') {
      files[path] = resolved;
      lazyMeta.push([path, mode]);
    } else {
      files[path] = { content: resolved, mode, mtime };
    }
  }

  const bash = new Bash({
    files,
    cwd: '/',
    env: { HOME: '/root', USER: 'root', HOSTNAME: 'bridgething', TERM: 'xterm' },
    customCommands: [uname],
  });
  const fs = bash.fs;

  for (const [path, mode] of lazyMeta) {
    await fs.chmod(path, mode).catch(() => {});
    if (mtime) await fs.utimes(path, mtime, mtime).catch(() => {});
  }

  for (const dir of manifest.dirs) {
    await fs.mkdir(dir, { recursive: true }).catch(() => {});
  }

  for (const name of getCommandNames()) {
    const path = `/usr/bin/${name}`;
    const entry = real.get(path);
    if (!entry) {
      await fs.rm(path, { force: true }).catch(() => {});
    } else {
      const resolved = resolveContent(entry[3], entry[1]);
      await fs.writeFile(path, typeof resolved === 'string' ? resolved : await resolved());
      await fs.chmod(path, entry[2]).catch(() => {});
    }
  }

  for (const [path, target] of manifest.links) {
    await fs.rm(path, { recursive: true, force: true }).catch(() => {});
    await fs.symlink(target, path).catch(() => {});
  }

  return bash;
}
