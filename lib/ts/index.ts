export * from './bindings/client';
export * from './bindings/gateway';
export * from './bindings/server';
export * from './bindings/shared';
export * from './bindings/stock';

export const BRIDGETHING_PROFILE_UUID = 'dead0000-854d-408e-81f0-fb6147f918fd';
export const BRIDGETHING_RFCOMM_CHANNEL = 1;
export const BRIDGETHING_SERVICE_UUID = 'dead0000-53e5-4085-a5d8-f55f3f14ac5a';
export const BRIDGETHING_CHARACTERISTIC_UUID = 'dead0000-f3dc-4620-8b74-8bd49bb5a468';
export const BRIDGETHING_MANUFACTURER_ID = 0xdead;

import { version } from './version';
export const LIB_VERSION = `v${version}`;
export const LIBBRIDGETHING_VERSION = 'v0.1.0';

export * from './logger';
