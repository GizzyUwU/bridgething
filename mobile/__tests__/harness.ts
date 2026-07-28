import type { FakeNative } from '../__mocks__/react-native-nitro-modules';

type Modules = {
  session: typeof import('../lib/session');
  storage: typeof import('../lib/storage');
  webapps: typeof import('../lib/webapps');
  ota: typeof import('../lib/ota');
  bridge: typeof import('../lib/bridge');
  catalog: typeof import('../lib/catalog');
  permissions: typeof import('react-native-permissions');
};

export type Rig = Modules & {
  native: FakeNative;
  emit(event: string, ...args: unknown[]): void;
  relaunch(): Rig;
};

const MMKV_KEYS = ['setup.completed', 'device.ledger', 'catalog.sources'];

export type RigOptions = { platform?: 'ios' | 'android' };

function build(opts: RigOptions, carry?: Record<string, string>): Rig {
  jest.resetModules();

  const rn = require('react-native') as typeof import('react-native');
  Object.defineProperty(rn.Platform, 'OS', {
    value: opts.platform ?? 'ios',
    configurable: true,
  });

  const nitro =
    require('../__mocks__/react-native-nitro-modules') as typeof import('../__mocks__/react-native-nitro-modules');
  nitro.resetNatives();

  const storage = require('../lib/storage') as Modules['storage'];
  if (carry)
    for (const [key, value] of Object.entries(carry))
      storage.storage.set(key, value);

  const bridge = require('../lib/bridge') as Modules['bridge'];
  const session = require('../lib/session') as Modules['session'];
  const webapps = require('../lib/webapps') as Modules['webapps'];
  const ota = require('../lib/ota') as Modules['ota'];
  const catalog = require('../lib/catalog') as Modules['catalog'];
  const permissions =
    require('react-native-permissions') as Modules['permissions'];

  session.registerSessionDomain();
  webapps.registerWebappsDomain();
  ota.registerOtaDomain();
  bridge.startBridge();

  const native = nitro.fakeNative();

  return {
    session,
    storage,
    webapps,
    ota,
    bridge,
    catalog,
    permissions,
    native,
    emit(event, ...args) {
      const handler = native.__handlers.get(event);
      if (!handler)
        throw new Error(`no native handler registered for "${event}"`);
      handler(...args);
    },
    relaunch() {
      const bytes: Record<string, string> = {};
      for (const key of MMKV_KEYS) {
        const value = storage.storage.getString(key);
        if (value !== undefined) bytes[key] = value;
      }
      return build(opts, bytes);
    },
  };
}

export function rig(opts: RigOptions = {}): Rig {
  return build(opts);
}
