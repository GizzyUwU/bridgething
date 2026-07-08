#!/usr/bin/env bun
// Build this webapp and install it onto a connected Car Thing: rsync dist/ to
// the device, then tell the daemon to switch the kiosk to it. You own this
// script; tweak it freely.
import { decode as msgpackDecode, encode as msgpackEncode } from '@msgpack/msgpack';
import { spawn } from 'node:child_process';
import { randomUUID } from 'node:crypto';
import { existsSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';

function parseUuid(s: string): Uint8Array {
  const hex = s.replace(/-/g, '').toLowerCase();
  if (hex.length !== 32 || !/^[0-9a-f]+$/.test(hex)) {
    throw new Error(`invalid uuid: ${s}`);
  }
  const out = new Uint8Array(16);
  for (let i = 0; i < 16; i++) out[i] = parseInt(hex.slice(i * 2, i * 2 + 2), 16);
  return out;
}

function freshMsgId(): Uint8Array {
  return parseUuid(randomUUID());
}

function uuidToString(bytes: Uint8Array): string {
  const hex = Array.from(bytes, b => b.toString(16).padStart(2, '0')).join('');
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}

const FRAME_HEADER_LENGTH = 16;
const FRAME_MAGIC = 0xdead;
const FRAME_VERSION = 2;
const COMPRESSION_NONE = 0x00;
const ENCODING_MSGPACK = 0x00;
const PRIORITY_NORMAL = 0x00;

function writeFrameHeader(payloadLength: number): Uint8Array<ArrayBuffer> {
  const buf = new Uint8Array(FRAME_HEADER_LENGTH);
  const view = new DataView(buf.buffer);
  view.setUint16(0, FRAME_MAGIC, false);
  view.setUint8(2, FRAME_VERSION);
  view.setUint8(3, COMPRESSION_NONE);
  view.setUint8(4, ENCODING_MSGPACK);
  view.setUint8(5, PRIORITY_NORMAL);
  view.setBigUint64(8, BigInt(payloadLength), false);
  return buf;
}

function frame(message: unknown): Uint8Array<ArrayBuffer> {
  const body = msgpackEncode(message);
  const header = writeFrameHeader(body.length);
  const out = new Uint8Array(header.length + body.length);
  out.set(header, 0);
  out.set(body, header.length);
  return out;
}

type GatewayMsg = { id: Uint8Array; meta: unknown; data: unknown };

class FrameAccumulator {
  private buffer = new Uint8Array(0);

  append(chunk: Uint8Array): void {
    if (chunk.length === 0) return;
    const merged = new Uint8Array(this.buffer.length + chunk.length);
    merged.set(this.buffer, 0);
    merged.set(chunk, this.buffer.length);
    this.buffer = merged;
  }

  next(): GatewayMsg | null {
    if (this.buffer.length < FRAME_HEADER_LENGTH) return null;
    const view = new DataView(this.buffer.buffer, this.buffer.byteOffset, this.buffer.byteLength);
    const magic = view.getUint16(0, false);
    if (magic !== FRAME_MAGIC) throw new Error(`bad framing magic 0x${magic.toString(16)}`);
    const version = view.getUint8(2);
    if (version !== FRAME_VERSION) throw new Error(`unsupported frame version ${version}`);
    const compression = view.getUint8(3);
    if (compression !== COMPRESSION_NONE) {
      throw new Error(`unsupported inbound compression ${compression} (this script only handles uncompressed)`);
    }
    const encoding = view.getUint8(4);
    if (encoding !== ENCODING_MSGPACK) throw new Error(`unsupported inbound encoding ${encoding}`);
    const len = Number(view.getBigUint64(8, false));
    const total = FRAME_HEADER_LENGTH + len;
    if (this.buffer.length < total) return null;
    const body = this.buffer.subarray(FRAME_HEADER_LENGTH, total);
    const decoded = msgpackDecode(body) as GatewayMsg;
    this.buffer = this.buffer.slice(total);
    return decoded;
  }
}

function bytesEqual(a: Uint8Array, b: Uint8Array): boolean {
  if (a.length !== b.length) return false;
  let diff = 0;
  for (let i = 0; i < a.length; i++) diff |= a[i] ^ b[i];
  return diff === 0;
}

type SwitchOutcome =
  | { ok: true; activeId: Uint8Array | null; activeName: string | null }
  | { ok: false; reason: string };

async function sendSwitchTo(host: string, port: number, manifestId: string): Promise<SwitchOutcome> {
  const url = `ws://${host}:${port}/`;
  console.log(`gateway ${url}`);
  const ws = new WebSocket(url);
  ws.binaryType = 'arraybuffer';
  const acc = new FrameAccumulator();

  const announce: GatewayMsg = {
    id: freshMsgId(),
    meta: { kind: 'event' },
    data: {
      type: 'capabilities',
      data: {
        event: 'announce',
        data: {
          gateway: {
            address: '',
            name: 'bridgething-webapp-push',
            osName: 'host',
            appName: 'bridgething-webapp-push',
            appVersion: '0.1.0',
            adapterVersion: 'host',
            libVersion: 'v0',
            libbridgethingVersion: 'v0',
          },
          uriSchemes: [],
          network: { kind: 'unknown', metered: false },
          available: { geo: false, notifications: false, netFetch: false, netWs: false, audioTts: false },
          audio: { earcons: [], voices: [] },
        },
      },
    },
  };

  const switchRequestId = freshMsgId();
  const switchMsg: GatewayMsg = {
    id: switchRequestId,
    meta: { kind: 'request' },
    data: {
      type: 'webapp',
      data: { event: 'switchTo', data: { id: parseUuid(manifestId) } },
    },
  };

  let switchSent = false;

  return await new Promise<SwitchOutcome>((res, rej) => {
    const overall = setTimeout(() => {
      try {
        ws.close();
      } catch {}
      rej(new Error('gateway switch timed out (15s)'));
    }, 15_000);

    ws.addEventListener('open', () => {
      ws.send(frame(announce));
      ws.send(frame(switchMsg));
      switchSent = true;
    });

    ws.addEventListener('message', (event: MessageEvent) => {
      const data = event.data;
      const bytes = data instanceof ArrayBuffer ? new Uint8Array(data) : data instanceof Uint8Array ? data : null;
      if (!bytes) return;
      try {
        acc.append(bytes);
        let msg = acc.next();
        while (msg !== null) {
          const meta = msg.meta as { kind?: string; data?: { requestId?: Uint8Array } };
          if (meta?.kind === 'response') {
            const respId = meta.data?.requestId;
            if (respId && bytesEqual(respId, switchRequestId)) {
              clearTimeout(overall);
              try {
                ws.close();
              } catch {}
              res(interpretSwitchResponse(msg.data));
              return;
            }
          }
          msg = acc.next();
        }
      } catch (err) {
        clearTimeout(overall);
        try {
          ws.close();
        } catch {}
        rej(err instanceof Error ? err : new Error(String(err)));
      }
    });

    ws.addEventListener('close', (event: CloseEvent) => {
      clearTimeout(overall);
      // the daemon activates the webapp and then drops this ephemeral connection;
      // a clean close after the switch was sent means it took, ack or not.
      if (switchSent && (event.code === 1000 || event.code === 1005)) {
        res({ ok: true, activeId: null, activeName: null });
        return;
      }
      rej(new Error(`gateway ws closed before response (code ${event.code})`));
    });
  });
}

function interpretSwitchResponse(data: unknown): SwitchOutcome {
  const outer = data as { type?: string; data?: unknown };
  if (outer?.type !== 'webapp') {
    return { ok: false, reason: `unexpected response type ${JSON.stringify(outer?.type)}` };
  }
  const inner = outer.data as { event?: string; data?: unknown };
  if (inner?.event === 'switched') {
    const active = inner.data as { id?: Uint8Array | null; name?: string | null } | null;
    return { ok: true, activeId: active?.id ?? null, activeName: active?.name ?? null };
  }
  if (inner?.event === 'webappError') {
    const errVariant = inner.data as { type?: string; data?: { id?: string; reason?: string } } | undefined;
    return { ok: false, reason: `daemon refused: ${errVariant?.type} ${JSON.stringify(errVariant?.data ?? {})}` };
  }
  return { ok: false, reason: `unexpected webapp response variant ${JSON.stringify(inner?.event)}` };
}

function rsync(localDir: string, host: string, remoteName: string): Promise<void> {
  const sshArgs = 'ssh -o UserKnownHostsFile=/dev/null -o StrictHostKeyChecking=no -o LogLevel=ERROR';
  const src = localDir.endsWith('/') ? localDir : `${localDir}/`;
  const dest = `root@${host}:/var/bridgething/webapps/${remoteName}/`;
  console.log(`rsync ${src} -> ${dest}`);
  return new Promise<void>((res, rej) => {
    const child = spawn('rsync', ['-avz', '--delete', '-e', sshArgs, src, dest], { stdio: 'inherit' });
    child.on('exit', code => (code === 0 ? res() : rej(new Error(`rsync exited ${code}`))));
    child.on('error', rej);
  });
}

function buildBundle(repoDir: string): Promise<void> {
  console.log('bun run build');
  return new Promise<void>((res, rej) => {
    const child = spawn('bun', ['run', 'build'], { cwd: repoDir, stdio: 'inherit' });
    child.on('exit', code => (code === 0 ? res() : rej(new Error(`bun run build exited ${code}`))));
    child.on('error', rej);
  });
}

async function main(): Promise<void> {
  const args = process.argv.slice(2);
  let skipBuild = process.env.SKIP_BUILD === '1';
  let switchAfter = true;
  let host = process.env.SUPERBIRD_HOST ?? 'bridgething.local';
  for (const arg of args) {
    if (arg === '--skip-build') skipBuild = true;
    else if (arg === '--no-switch') switchAfter = false;
    else if (arg.startsWith('--')) throw new Error(`unknown flag: ${arg}`);
    else host = arg;
  }
  const port = Number(process.env.BRIDGETHING_GATEWAY_PORT ?? 8892);

  const repoDir = resolve(import.meta.dir, '..');
  const distDir = resolve(repoDir, 'dist');
  const manifestPath = resolve(distDir, 'manifest.json');

  if (!skipBuild) await buildBundle(repoDir);

  if (!existsSync(manifestPath)) {
    throw new Error(`no manifest.json at ${manifestPath}; run 'bun run build' first or drop --skip-build`);
  }
  const manifest = JSON.parse(readFileSync(manifestPath, 'utf8')) as { id?: string };
  if (!manifest.id) throw new Error(`${manifestPath} has no 'id' field`);

  const remoteName = process.env.BRIDGETHING_BUNDLE_NAME ?? manifest.id;
  await rsync(distDir, host, remoteName);

  if (!switchAfter) {
    console.log('skipping switch (--no-switch)');
    return;
  }

  const outcome = await sendSwitchTo(host, port, manifest.id);
  if (!outcome.ok) throw new Error(outcome.reason);
  const activeStr = outcome.activeId ? uuidToString(outcome.activeId) : '(none)';
  console.log(`active webapp: ${outcome.activeName ?? '(unnamed)'} ${activeStr}`);
}

main().catch(err => {
  console.error(err instanceof Error ? err.message : err);
  process.exit(1);
});
