type Callback = (...args: unknown[]) => void;

export type FakeNative = Record<string, unknown> & {
  __handlers: Map<string, Callback>;
  __returns: Map<string, unknown>;
  __calls: string[];
};

const natives = new Map<string, FakeNative>();

function handlerKey(method: string): string {
  const rest = method.slice('setOn'.length);
  return rest.charAt(0).toLowerCase() + rest.slice(1);
}

function makeNative(name: string): FakeNative {
  const handlers = new Map<string, Callback>();
  const returns = new Map<string, unknown>();
  const calls: string[] = [];
  const target = {
    __handlers: handlers,
    __returns: returns,
    __calls: calls,
  } as FakeNative;

  return new Proxy(target, {
    get(obj, prop: string) {
      if (prop in obj) return obj[prop as keyof FakeNative];
      if (prop === 'then') return undefined; // never look thenable to await
      if (prop.startsWith('setOn')) {
        return (cb: Callback) => handlers.set(handlerKey(prop), cb);
      }
      return (...args: unknown[]) => {
        calls.push(prop);
        const canned = returns.get(prop);
        if (typeof canned === 'function')
          return (canned as (...a: unknown[]) => unknown)(...args);
        return Promise.resolve(canned);
      };
    },
  }) as FakeNative;
}

export const NitroModules = {
  createHybridObject<T>(name: string): T {
    let native = natives.get(name);
    if (!native) {
      native = makeNative(name);
      natives.set(name, native);
    }
    return native as unknown as T;
  },
};

export function fakeNative(name = 'BridgethingSession'): FakeNative {
  return NitroModules.createHybridObject<FakeNative>(name);
}

export function resetNatives(): void {
  natives.clear();
}
