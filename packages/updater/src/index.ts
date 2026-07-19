export type { OtaKind, OtaPhase } from '@bridgething/gateway';

export { AckRegistry, AckWindow, TransferStalledError } from './ack-window.js';
export type { AckWindowOptions } from './ack-window.js';
export { OtaDriver } from './driver.js';
export type { OtaProgressSnapshot, ProgressListener, WebappInstallResult } from './driver.js';
export { DEFAULT_FRAGMENT_BYTES, streamRangeFragments, streamSourceFragments } from './fragments.js';
export type { StreamRangeOptions, StreamSourceOptions } from './fragments.js';
export type { GatewayDevice } from './gateway-device.js';
export { serveOtaAssetRanges } from './range-serve.js';
export { blobArtifactSource, bytesArtifactSource, sha256Hex } from './source.js';
export type { ArtifactSource } from './source.js';
