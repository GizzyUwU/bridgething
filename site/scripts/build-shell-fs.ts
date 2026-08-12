import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { existsSync, lstatSync, mkdtempSync, readdirSync, readFileSync, readlinkSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { basename, join, resolve } from 'node:path';

const INLINE_TEXT_BYTES = 32 * 1024;
const CONTENT_BINARY = -1;
const CONTENT_ELF = -2;
const CONTENT_TEXT_OMITTED = -3;

const CONTENT_EXCLUDED_DIRS = [
  '/usr/share/mime',
  '/usr/share/xkeyboard-config-2',
  '/usr/share/alsa',
  '/usr/share/X11',
  '/usr/share/ca-certificates',
  '/usr/share/zoneinfo',
  '/usr/share/terminfo',
  '/usr/share/consolefonts',
  '/etc/ssl',
  '/etc/ssh',
];

const KEY_NAME = /(\.pem|\.key|_key|shadow|gshadow)$/;
const KEY_CONTENT = /-----BEGIN [A-Z ]*PRIVATE KEY-----/;

type FileRow = [path: string, size: number, mode: number, content: number | string];
type LinkRow = [path: string, target: string];

function extractExt4(image: string): string {
  const debugfs = ['/opt/homebrew/opt/e2fsprogs/sbin/debugfs', 'debugfs'].find(p => spawnSync(p, ['-V']).status === 0);
  if (!debugfs) throw new Error('debugfs not found (brew install e2fsprogs)');
  const out = mkdtempSync(join(tmpdir(), 'shell-fs-'));
  spawnSync(debugfs, ['-R', `rdump / ${out}`, image], { stdio: 'ignore' });
  if (!existsSync(join(out, 'etc')) || !existsSync(join(out, 'usr'))) {
    throw new Error(`debugfs extraction produced no tree in ${out}`);
  }
  return out;
}

function isText(buf: Buffer): boolean {
  const probe = buf.subarray(0, 4096);
  if (probe.includes(0)) return false;
  let printable = 0;
  for (const b of probe) if (b === 9 || b === 10 || b === 13 || (b >= 32 && b < 127) || b >= 128) printable++;
  return probe.length === 0 || printable / probe.length > 0.95;
}

function contentExcluded(path: string): boolean {
  return CONTENT_EXCLUDED_DIRS.some(d => path === d || path.startsWith(`${d}/`));
}

const input = process.argv[2];
if (!input) throw new Error('usage: bun run scripts/build-shell-fs.ts <image.ext4 | rootfs-dir>');
const resolved = resolve(input);
if (!existsSync(resolved)) throw new Error(`no such input: ${resolved}`);

const isImage = lstatSync(resolved).isFile();
const root = isImage ? extractExt4(resolved) : resolved;

const dirs: string[] = [];
const links: LinkRow[] = [];
const files: FileRow[] = [];
const texts: string[] = [];
const textIndex = new Map<string, number>();
const blobDir = resolve(import.meta.dirname, '..', 'public', 'shell-fs');
const blobs = new Map<string, string>();
let contentBytes = 0;
let blobBytes = 0;
let skippedKeys = 0;

rmSync(blobDir, { recursive: true, force: true });

function lazyBlob(content: string): string {
  const hash = createHash('sha256').update(content).digest('hex').slice(0, 12);
  if (!blobs.has(hash)) {
    blobs.set(hash, content);
    blobBytes += Buffer.byteLength(content);
  }
  return hash;
}

function walk(rel: string): void {
  const abs = join(root, rel);
  for (const name of readdirSync(abs).sort()) {
    const path = `${rel}/${name}`;
    const st = lstatSync(join(root, path));
    if (st.isSymbolicLink()) {
      links.push([path, readlinkSync(join(root, path))]);
    } else if (st.isDirectory()) {
      dirs.push(path);
      walk(path);
    } else if (st.isFile()) {
      let content: number | string = CONTENT_BINARY;
      if (st.size > 0) {
        const buf = readFileSync(join(root, path));
        if (buf.length >= 4 && buf[0] === 0x7f && buf[1] === 0x45 && buf[2] === 0x4c && buf[3] === 0x46) {
          content = CONTENT_ELF;
        } else if (isText(buf)) {
          const s = buf.toString('utf-8');
          if (KEY_NAME.test(name) || KEY_CONTENT.test(s)) {
            content = CONTENT_TEXT_OMITTED;
            skippedKeys++;
          } else if (st.size <= INLINE_TEXT_BYTES && !contentExcluded(path)) {
            let idx = textIndex.get(s);
            if (idx === undefined) {
              idx = texts.length;
              texts.push(s);
              textIndex.set(s, idx);
              contentBytes += buf.length;
            }
            content = idx;
          } else {
            content = lazyBlob(s);
          }
        }
      }
      files.push([path, st.size, st.mode & 0o7777, content]);
    }
  }
}

walk('');

const DEMO_SERIAL = 'SB2202C0FFEE';
const DEMO_BT_MAC = 'DE:CA:FB:C0:FF:EE';

function addText(content: string): number {
  let idx = textIndex.get(content);
  if (idx === undefined) {
    idx = texts.length;
    texts.push(content);
    textIndex.set(content, idx);
    contentBytes += Buffer.byteLength(content);
  }
  return idx;
}

function addFile(path: string, content: string, mode: number): void {
  files.push([path, Buffer.byteLength(content), mode, addText(content)]);
}

function addPlaceholder(path: string, note: string, mode: number): void {
  files.push([path, Buffer.byteLength(note), mode, addText(note)]);
}

function synthesizeRuntime(): void {
  for (const dir of [
    '/run/systemd',
    '/run/systemd/network',
    '/var/bridgething',
    '/var/bridgething/webapps',
    '/var/lib/bandaid',
    '/var/lib/bluetooth',
    `/var/lib/bluetooth/${DEMO_BT_MAC}`,
    '/var/lib/bridgething',
    '/var/lib/bridgething/state',
    '/var/lib/bridgething/state/assets',
    '/var/lib/bridgething/state/range-spool',
    '/var/lib/bridgething/state/transfers',
    '/var/lib/ssh',
    '/var/lib/superbird',
    '/var/lib/timezone',
  ]) {
    dirs.push(dir);
  }

  const templateRow = files.find(f => f[0] === '/usr/share/superbird/meta.json.in');
  const templateIdx = templateRow && typeof templateRow[3] === 'number' && templateRow[3] >= 0 ? templateRow[3] : null;
  const template = templateIdx === null ? null : texts[templateIdx]!;
  if (template) {
    const meta = template
      .replace('"btMac": ""', `"btMac": "${DEMO_BT_MAC}"`)
      .replace('"serialNumber": ""', `"serialNumber": "${DEMO_SERIAL}"`);
    addFile('/var/lib/superbird/meta.json', meta, 0o644);
  }

  addFile(
    `/var/lib/bluetooth/${DEMO_BT_MAC}/settings`,
    `[General]\nAlias=Car Thing (SN: ${DEMO_SERIAL.slice(-4)})\n`,
    0o600,
  );

  const serialSha = createHash('sha256').update(DEMO_SERIAL).digest('hex');
  const subnetOffset = (parseInt(serialSha.slice(0, 2), 16) & 0x1f) * 8;
  addFile(
    '/run/systemd/network/11-usb-ncm.network',
    [
      '[Match]',
      'Name=uncm*',
      '',
      '[Network]',
      `Address=10.42.1.${subnetOffset + 2}/29`,
      'DHCPServer=yes',
      'LinkLocalAddressing=no',
      'IPv6AcceptRA=no',
      'IPMasquerade=no',
      'ConfigureWithoutCarrier=yes',
      'EmitLLDP=no',
      '',
      '[DHCPServer]',
      'PoolOffset=3',
      'PoolSize=4',
      'EmitDNS=no',
      'EmitNTP=no',
      'EmitRouter=no',
      '',
      '[Link]',
      'RequiredForOnline=no',
      '',
    ].join('\n'),
    0o644,
  );

  for (const algo of ['rsa', 'ecdsa', 'ed25519']) {
    addPlaceholder(
      `/var/lib/ssh/ssh_host_${algo}_key`,
      'generated per-device at first boot by sshdgenkeys. not shipped, obviously.\n',
      0o600,
    );
    addPlaceholder(
      `/var/lib/ssh/ssh_host_${algo}_key.pub`,
      'generated per-device at first boot by sshdgenkeys.\n',
      0o644,
    );
  }

  addFile('/var/lib/timezone/timezone', 'UTC\n', 0o644);
  links.push(['/var/lib/timezone/localtime', '/usr/share/zoneinfo/UTC']);

  const factoryPrefix = '/usr/lib/bridgething';
  const mirrorPrefixes = ['/var/lib/bandaid/bridgething', '/opt/bridgething'];
  for (const dir of [...dirs]) {
    if (dir.startsWith(`${factoryPrefix}/`)) {
      for (const prefix of mirrorPrefixes) dirs.push(dir.replace(factoryPrefix, prefix));
    }
  }
  dirs.push('/var/lib/bandaid/bridgething');
  for (const row of [...files]) {
    if (row[0].startsWith(`${factoryPrefix}/`)) {
      for (const prefix of mirrorPrefixes) files.push([row[0].replace(factoryPrefix, prefix), row[1], row[2], row[3]]);
    }
  }

  const imageVersion = template ? /"imageVersion": "([^"]+)"/.exec(template)?.[1] : undefined;
  for (const prefix of mirrorPrefixes) {
    addFile(`${prefix}/.adopted-image-version`, `${imageVersion ?? 'unknown'}\n`, 0o644);
  }

  addPlaceholder(
    '/var/lib/bridgething/state/bridgething.db',
    'sqlite database: paired devices, key-value store, last-active webapp. per-device state, not shipped.\n',
    0o644,
  );
}

synthesizeRuntime();

const manifest = {
  image: isImage ? basename(resolved) : basename(root),
  dirs,
  links,
  files,
  texts,
};

const out = resolve(import.meta.dirname, '..', 'public', 'shell-fs.json');
await Bun.write(out, JSON.stringify(manifest));
for (const [hash, content] of blobs) {
  await Bun.write(join(blobDir, `${hash}.txt`), content);
}
if (isImage) rmSync(root, { recursive: true, force: true });

const inline = files.filter(f => typeof f[3] === 'number' && f[3] >= 0).length;
const lazy = files.filter(f => typeof f[3] === 'string').length;
console.log(
  `shell-fs.json: ${dirs.length} dirs, ${files.length} files (${inline} inline, ${lazy} lazy, ${skippedKeys} key-like skipped), ${links.length} symlinks`,
);
console.log(
  `inline text: ${(contentBytes / 1024).toFixed(0)} KiB raw, manifest ${((await Bun.file(out).arrayBuffer()).byteLength / 1048576).toFixed(2)} MB, lazy blobs: ${blobs.size} files, ${(blobBytes / 1048576).toFixed(1)} MB`,
);
