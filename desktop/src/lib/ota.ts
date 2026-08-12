import type { OtaAvailable, OtaPollConfig, OtaRun, OtaRunPhase } from '@bridgething/companion-types';
import type { Tone } from '@bridgething/ui';

const DEFAULT_OTA_ROOT_URL = 'https://ota.bridgething.com';

export const DEFAULT_OTA_POLL_CONFIG: OtaPollConfig = {
  intervalSeconds: 3600,
  autoPush: true,
  rootUrl: DEFAULT_OTA_ROOT_URL,
};

export const POLL_INTERVALS = ['900', '3600', '21600', '86400'] as const;

export function intervalLabel(seconds: number): string {
  if (seconds % 86_400 === 0) return `${seconds / 86_400}d`;
  if (seconds % 3600 === 0) return `${seconds / 3600}h`;
  return `${Math.round(seconds / 60)}m`;
}

export function rootUrlOf(config: OtaPollConfig | null | undefined): string {
  const held = config?.rootUrl?.trim();
  return held && held.length > 0 ? held : DEFAULT_OTA_ROOT_URL;
}

export function isRunning(run: OtaRun | undefined): run is OtaRun {
  return run !== undefined && run.outcome === null;
}

export function hasActivity(run: OtaRun | undefined, available: OtaAvailable | undefined): boolean {
  if (run && run.outcome !== 'succeeded') return true;
  return Boolean(available?.releaseVersion);
}

export function stepIndex(run: OtaRun): number {
  return Math.max(
    0,
    run.steps.findIndex(step => step.id === run.stepId),
  );
}

export function stepLabel(run: OtaRun): string | null {
  return run.steps[stepIndex(run)]?.label ?? null;
}

export function stepPercent(run: OtaRun): number | null {
  if (run.outcome === 'succeeded') return 100;
  if (run.phase === 'writing' || run.phase === 'confirming') {
    return run.dwlPercent === null ? null : Math.max(0, Math.min(100, Math.round(run.dwlPercent)));
  }
  const total = run.stageTotal ?? 0;
  if (total <= 0) return null;
  return Math.max(0, Math.min(100, Math.round(((run.stageReceived ?? 0) / total) * 100)));
}

const PHASE_WORDS: Record<OtaRunPhase, string> = {
  idle: 'queued',
  downloading: 'downloading',
  streaming: 'sending to the device',
  verifying: 'verifying',
  writing: 'writing',
  confirming: 'confirming',
  reboot: 'rebooting',
  completed: 'done',
  failed: 'failed',
};

export function phaseWord(run: OtaRun): string {
  if (run.outcome === 'cancelled') return 'cancelled';
  if (run.outcome === 'failed') return 'failed';
  if (run.outcome === 'succeeded') return 'done';
  return PHASE_WORDS[run.phase];
}

export function runTone(run: OtaRun): Tone {
  if (run.outcome === 'succeeded') return 'ok';
  if (run.outcome !== null) return 'err';
  return 'accent';
}

export function runTitle(run: OtaRun): string {
  if (run.kind === 'installedWebapp' || run.kind === 'builtinWebapp') {
    return run.webappName ?? 'webapp';
  }
  if (run.kind === 'wakewordModel') return 'wakeword model';
  if (run.kind === 'daemon') return `daemon ${run.daemonVersion ?? ''}`.trim();
  return run.releaseVersion ? `release ${run.releaseVersion}` : 'system update';
}
