import { expect, test } from 'bun:test';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { DeliveryClient } from '../index.js';

const enabled = process.env.BRIDGETHING_NAPI_E2E === '1';
const repoRoot = resolve(import.meta.dir, '../../../..');
const url = 'ws://127.0.0.1:8892/';
const deadlineMs = 180_000;

const artifactBytes = 512 * 1024;

function devDaemon(action: 'start' | 'stop') {
  const result = spawnSync(join(repoRoot, 'scripts/dev-daemon.sh'), [action], { cwd: repoRoot, stdio: 'inherit' });
  if (result.status !== 0) throw new Error(`dev-daemon.sh ${action} exited ${result.status}`);
}

function writeArtifact(dir: string, name: string, length: number) {
  const path = join(dir, name);
  const body = Buffer.alloc(length);
  for (let at = 0; at < length; at += 1) body[at] = at % 251;
  writeFileSync(path, body);
  return path;
}

function crc32(bytes: Uint8Array) {
  let crc = ~0;
  for (const byte of bytes) {
    crc ^= byte;
    for (let bit = 0; bit < 8; bit += 1) crc = (crc >>> 1) ^ (0xedb88320 & -(crc & 1));
  }
  return ~crc >>> 0;
}

function writeBundle(id: string) {
  const files = [
    { name: 'index.html', body: Buffer.from('<!doctype html><title>e2e</title>') },
    {
      name: 'manifest.json',
      body: Buffer.from(JSON.stringify({ id, name: 'e2e', version: '0.1.0', config: [], permissions: [] })),
    },
  ];
  const entries: Buffer[] = [];
  const central: Buffer[] = [];
  let offset = 0;

  for (const file of files) {
    const name = Buffer.from(file.name);
    const crc = crc32(file.body);

    const local = Buffer.alloc(30 + name.length);
    local.writeUInt32LE(0x04034b50, 0);
    local.writeUInt16LE(20, 4);
    local.writeUInt32LE(crc, 14);
    local.writeUInt32LE(file.body.length, 18);
    local.writeUInt32LE(file.body.length, 22);
    local.writeUInt16LE(name.length, 26);
    name.copy(local, 30);
    entries.push(local, file.body);

    const header = Buffer.alloc(46 + name.length);
    header.writeUInt32LE(0x02014b50, 0);
    header.writeUInt16LE(20, 4);
    header.writeUInt16LE(20, 6);
    header.writeUInt32LE(crc, 16);
    header.writeUInt32LE(file.body.length, 20);
    header.writeUInt32LE(file.body.length, 24);
    header.writeUInt16LE(name.length, 28);
    header.writeUInt32LE(offset, 42);
    name.copy(header, 46);
    central.push(header);

    offset += local.length + file.body.length;
  }

  const directory = Buffer.concat(central);
  const end = Buffer.alloc(22);
  end.writeUInt32LE(0x06054b50, 0);
  end.writeUInt16LE(files.length, 8);
  end.writeUInt16LE(files.length, 10);
  end.writeUInt32LE(directory.length, 12);
  end.writeUInt32LE(offset, 16);
  return Buffer.concat([...entries, directory, end]);
}

test.if(enabled)(
  'a push and an install both complete against the dev daemon',
  async () => {
    devDaemon('start');
    try {
      const client = await DeliveryClient.connect(url, { deviceId: 'napi-e2e' });
      expect(client.deviceId()).toBe('napi-e2e');

      const spool = mkdtempSync(join(tmpdir(), 'bridgething-napi-e2e-'));
      const terminal = await client.push('daemon', writeArtifact(spool, 'daemon', artifactBytes));
      expect(terminal.kind).toBe('completed');

      const id = crypto.randomUUID();
      const installed = await client.installWebapp(writeBundle(id), 'https://apps.bridgething.test/catalog.json');
      expect(installed.id).toBe(id);
      expect(installed.name).toBe('e2e');

      expect(await client.switchWebapp(id)).toBe(id);
    } finally {
      devDaemon('stop');
    }
  },
  deadlineMs,
);
