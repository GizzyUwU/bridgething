import type {
  BridgethingDeviceMeta,
  BridgethingOtaRun,
  BridgethingSessionPeer,
  BridgethingSessionSnapshot,
} from '@bridgething/session-react-native';

export const DEVICE = 'aa:bb:cc:dd:ee:ff';
export const OTHER = '11:22:33:44:55:66';

export function peer(
  id = DEVICE,
  over: Partial<BridgethingSessionPeer> = {},
): BridgethingSessionPeer {
  return { id, name: 'Car Thing', status: 'connected', ...over };
}

export function meta(
  over: Partial<BridgethingDeviceMeta> = {},
): BridgethingDeviceMeta {
  return {
    daemonVersion: '0.6.0',
    libbridgethingVersion: '0.6.0',
    imageVersion: '2026.7.1',
    appName: 'bridgething',
    osName: 'superbird',
    osVersion: '1.0',
    channel: 'stable',
    modelName: 'Superbird',
    serialNumber: 'SN12345',
    ...over,
  };
}

export const EPOCH = 1_700_000_000_000;

export const IMAGE_STEPS = [
  {
    id: 0,
    kind: 'download' as const,
    label: 'downloading',
    bytes: 100_000_000,
  },
  { id: 1, kind: 'apply' as const, label: 'writing', bytes: 100_000_000 },
  { id: 2, kind: 'reboot' as const, label: 'rebooting', bytes: 0 },
];

export function otaRun(
  over: Partial<BridgethingOtaRun> = {},
): BridgethingOtaRun {
  return {
    runId: 'run-1',
    deviceId: DEVICE,
    otaKind: 'image',
    phase: 'downloading',
    steps: IMAGE_STEPS,
    stepId: 0,
    startedAt: EPOCH,
    phaseStartedAt: EPOCH,
    stageReceived: 50_000_000,
    stageTotal: 100_000_000,
    ratePerSec: 1_000_000,
    ...over,
  };
}

export function snapshot(
  peers: BridgethingSessionPeer[],
  metas: Record<string, BridgethingDeviceMeta> = {},
): BridgethingSessionSnapshot {
  return {
    hostInfo: {
      appName: 'bridgething',
      appVersion: '0.6.0',
      osName: 'iOS',
      osVersion: '26.0',
      hostIdentifier: 'host',
      libVersion: '0.6.0',
      libbridgethingVersion: '0.6.0',
      adapterVersion: '0.6.0',
    },
    providers: [],
    providerPriority: [],
    peers,
    ancsAuthStatuses: [],
    deviceMeta: Object.entries(metas).map(([deviceId, m]) => ({
      deviceId,
      meta: m,
    })),
    capabilityFlags: {
      geo: true,
      notifications: true,
      netFetch: true,
      netWs: true,
      audioTts: true,
      voiceModel: true,
    },
    voiceModel: { status: 'absent', receivedBytes: 0, totalBytes: 0 },
    webapps: [],
    otaRuns: [],
    otaAvailable: [],
    otaPoll: {},
  };
}
