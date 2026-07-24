import type { BridgethingGateway, OtaKind, OtaPatch, OtaPhase, WebappInfo } from '@bridgething/gateway';
import { newUuid } from '@bridgething/lib/uuid';

import type { AckWindowOptions } from './ack-window.js';
import { AckRegistry } from './ack-window.js';
import { DEFAULT_FRAGMENT_BYTES, streamSourceFragments } from './fragments.js';
import type { GatewayDevice } from './gateway-device.js';
import { serveOtaAssetRanges } from './range-serve.js';
import type { ArtifactSource } from './source.js';
import { sha256Hex } from './source.js';

const OTA_IDLE_DEADLINE_MS = 60_000;
const OTA_IDLE_CHECK_INTERVAL_MS = 15_000;

export type OtaProgressSnapshot =
  | { phase: 'streaming'; percent: number }
  | { phase: 'applying'; otaPhase: OtaPhase; percent: number }
  | { phase: 'staged' }
  | { phase: 'completed' }
  | { phase: 'failed'; reason: string };

export type ProgressListener = (snapshot: OtaProgressSnapshot) => void;

export type WebappInstallResult = { ok: true; info: WebappInfo } | { ok: false; reason: string };

type BandaidArtifact = { kind: OtaKind; source: ArtifactSource; patch?: OtaPatch };

export class OtaDriver {
  private readonly device: GatewayDevice;
  private readonly registry: AckRegistry;
  private readonly unsubscribeAck: () => void;
  private unsubscribeRangeServer: (() => void) | null = null;

  constructor(gateway: BridgethingGateway, deviceId: string, ackWindowOptions: AckWindowOptions = {}) {
    this.device = gateway.device(deviceId);
    this.registry = new AckRegistry(ackWindowOptions);
    this.unsubscribeAck = this.device.transfer.onAck(ack => this.registry.note(ack.transferId, ack.received));
  }

  serveAssetRanges(zcks: Map<string, ArtifactSource>): void {
    this.unsubscribeRangeServer?.();
    this.unsubscribeRangeServer = serveOtaAssetRanges(this.device, this.registry, zcks);
  }

  stopServingAssetRanges(): void {
    this.unsubscribeRangeServer?.();
    this.unsubscribeRangeServer = null;
  }

  close(): void {
    this.unsubscribeAck();
    this.stopServingAssetRanges();
  }

  async pushImage(opts: {
    source: ArtifactSource;
    zcks?: Map<string, ArtifactSource>;
    updateUrlBase?: string;
    onProgress?: ProgressListener;
  }): Promise<OtaProgressSnapshot> {
    if (opts.zcks) this.serveAssetRanges(opts.zcks);
    const { snapshot } = await this.driveOta({
      kind: 'image',
      source: opts.source,
      updateUrlBase: opts.updateUrlBase,
      mode: 'full',
      onProgress: opts.onProgress,
    });
    return snapshot;
  }

  async pushDaemon(source: ArtifactSource, onProgress?: ProgressListener, patch?: OtaPatch): Promise<OtaProgressSnapshot> {
    return this.applyBandaidBatch([{ kind: 'daemon', source, patch }], onProgress);
  }

  async pushBuiltinWebapp(source: ArtifactSource, onProgress?: ProgressListener): Promise<OtaProgressSnapshot> {
    return this.applyBandaidBatch([{ kind: 'builtinWebapp', source }], onProgress);
  }

  async pushBandaidBatch(artifacts: BandaidArtifact[], onProgress?: ProgressListener): Promise<OtaProgressSnapshot> {
    return this.applyBandaidBatch(artifacts, onProgress);
  }

  async installWebapp(source: ArtifactSource): Promise<WebappInstallResult> {
    const totalSize = source.size;
    const sha256 = await sha256Hex(source);
    const transferId = newUuid();

    const beginResult = await this.device.system.otaBegin({
      kind: 'installedWebapp',
      updateId: sha256,
      updateUrlBase: null,
      transfer: { id: transferId, totalSize, sha256 },
      patch: null,
    });
    if (!beginResult.ok) {
      return { ok: false, reason: describeBeginFailure(beginResult) };
    }
    const resumeFromOffset = beginResult.response.resumeFromOffset;

    const outcome = new Promise<WebappInstallResult>(resolve => {
      let settled = false;
      const finish = (result: WebappInstallResult) => {
        if (settled) return;
        settled = true;
        unsubInstalled();
        unsubError();
        resolve(result);
      };
      const unsubInstalled = this.device.webapp.onWebappInstalled(info => finish({ ok: true, info }));
      const unsubError = this.device.system.onOtaError(err =>
        finish({ ok: false, reason: `[${err.code}] ${err.msg}` }),
      );
    });

    const window = this.registry.register(transferId, resumeFromOffset);
    try {
      await streamSourceFragments({
        device: this.device,
        transferId,
        source,
        startOffset: resumeFromOffset,
        totalSize,
        chunkSize: DEFAULT_FRAGMENT_BYTES,
        priority: 'background',
        window,
      });
    } catch (err) {
      this.registry.deregister(transferId);
      return { ok: false, reason: `chunk stream failed: ${errorMessage(err)}` };
    }

    const result = await outcome;
    this.registry.deregister(transferId);
    return result;
  }

  private async applyBandaidBatch(
    artifacts: BandaidArtifact[],
    onProgress?: ProgressListener,
  ): Promise<OtaProgressSnapshot> {
    const stagedIds: string[] = [];
    for (const artifact of artifacts) {
      const { snapshot, updateId } = await this.driveOta({
        kind: artifact.kind,
        source: artifact.source,
        updateUrlBase: undefined,
        mode: 'stage',
        patch: artifact.patch,
        onProgress,
      });
      if (snapshot.phase !== 'staged') return snapshot;
      stagedIds.push(updateId);
    }
    return this.commitBandaid(stagedIds, onProgress);
  }

  private async commitBandaid(expected: string[], onProgress?: ProgressListener): Promise<OtaProgressSnapshot> {
    const terminal = this.awaitTerminal('full', onProgress);
    try {
      await this.device.system.otaActivate({ expected });
    } catch (err) {
      terminal.cancel();
      return { phase: 'failed', reason: `OtaActivate send failed: ${errorMessage(err)}` };
    }
    return terminal.promise;
  }

  private async driveOta(args: {
    kind: OtaKind;
    source: ArtifactSource;
    updateUrlBase?: string;
    mode: DriveMode;
    patch?: OtaPatch | null;
    onProgress?: ProgressListener;
  }): Promise<{ snapshot: OtaProgressSnapshot; updateId: string }> {
    const totalSize = args.source.size;
    const sha256 = await sha256Hex(args.source);
    const transferId = newUuid();

    const beginResult = await this.device.system.otaBegin({
      kind: args.kind,
      updateId: sha256,
      updateUrlBase: args.updateUrlBase ?? null,
      transfer: { id: transferId, totalSize, sha256 },
      patch: args.patch ?? null,
    });
    if (!beginResult.ok) {
      return { snapshot: { phase: 'failed', reason: describeBeginFailure(beginResult) }, updateId: sha256 };
    }
    const resumeFromOffset = beginResult.response.resumeFromOffset;
    args.onProgress?.({ phase: 'streaming', percent: percentOf(resumeFromOffset, totalSize) });

    const window = this.registry.register(transferId, resumeFromOffset);
    const terminal = this.awaitTerminal(args.mode, args.onProgress);
    const abort = new AbortController();

    const snapshot = await new Promise<OtaProgressSnapshot>(resolve => {
      let settled = false;
      const finish = (s: OtaProgressSnapshot) => {
        if (settled) return;
        settled = true;
        resolve(s);
      };
      void terminal.promise.then(finish);
      void (async () => {
        try {
          await streamSourceFragments({
            device: this.device,
            transferId,
            source: args.source,
            startOffset: resumeFromOffset,
            totalSize,
            chunkSize: DEFAULT_FRAGMENT_BYTES,
            priority: 'background',
            window,
            signal: abort.signal,
          });
        } catch (err) {
          if (!abort.signal.aborted) finish({ phase: 'failed', reason: `chunk stream failed: ${errorMessage(err)}` });
        }
      })();
    });

    abort.abort();
    terminal.cancel();
    this.registry.deregister(transferId);
    if (snapshot.phase === 'failed') {
      await this.device.transfer.abandon({ transferId, reason: 'attempt ended' }).catch(() => {});
    }
    return { snapshot, updateId: sha256 };
  }

  private awaitTerminal(
    mode: DriveMode,
    onProgress?: ProgressListener,
  ): { promise: Promise<OtaProgressSnapshot>; cancel: () => void } {
    const success: OtaProgressSnapshot = mode === 'full' ? { phase: 'completed' } : { phase: 'staged' };
    let settle!: (s: OtaProgressSnapshot) => void;
    const promise = new Promise<OtaProgressSnapshot>(resolve => {
      settle = resolve;
    });

    let lastProgressAt = Date.now();
    let finished = false;
    const finish = (s: OtaProgressSnapshot) => {
      if (finished) return;
      finished = true;
      cleanup();
      settle(s);
    };

    const unsubProgress = this.device.system.onOtaProgress(ev => {
      lastProgressAt = Date.now();
      onProgress?.({ phase: 'applying', otaPhase: ev.phase, percent: ev.percent });
      const done = mode === 'full' ? ev.phase === 'reboot' : ev.phase === 'writing' && ev.percent >= 100;
      if (done) finish(success);
    });
    const unsubError = this.device.system.onOtaError(ev =>
      finish({ phase: 'failed', reason: `[${ev.code}] ${ev.msg}` }),
    );
    const idleTimer = setInterval(() => {
      if (Date.now() - lastProgressAt > OTA_IDLE_DEADLINE_MS) {
        finish({ phase: 'failed', reason: `ota stalled: no progress within ${OTA_IDLE_DEADLINE_MS / 1000}s` });
      }
    }, OTA_IDLE_CHECK_INTERVAL_MS);

    function cleanup() {
      unsubProgress();
      unsubError();
      clearInterval(idleTimer);
    }

    return {
      promise,
      cancel: () => {
        if (finished) return;
        finished = true;
        cleanup();
      },
    };
  }
}

type DriveMode = 'full' | 'stage';

function percentOf(n: number, d: number): number {
  if (d === 0) return 100;
  return Math.min(100, Math.floor((n * 100) / d));
}

function describeBeginFailure(result: { ok: false; kind: 'domain' | 'protocol'; error: unknown }): string {
  if (result.kind === 'domain') {
    const err = result.error as { reason?: string };
    return `daemon rejected OtaBegin: ${err.reason ?? 'unknown reason'}`;
  }
  return `OtaBegin protocol error: ${JSON.stringify(result.error)}`;
}

function errorMessage(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}
