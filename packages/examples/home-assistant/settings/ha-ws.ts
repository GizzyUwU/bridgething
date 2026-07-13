export type HaEntity = { entityId: string; state: string; attributes: Record<string, unknown> };

export class HaAuthError extends Error {}
export class HaConnectError extends Error {}

const OPEN_TIMEOUT_MS = 15_000;

export function friendlyName(e: HaEntity): string {
  const fn = e.attributes['friendly_name'];
  if (typeof fn === 'string' && fn.length) return fn;
  const dot = e.entityId.indexOf('.');
  return (dot < 0 ? e.entityId : e.entityId.slice(dot + 1)).replace(/_/g, ' ');
}

export function domainOf(entityId: string): string {
  const dot = entityId.indexOf('.');
  return dot < 0 ? entityId : entityId.slice(0, dot);
}

export function fetchStates(baseUrl: string, token: string): Promise<HaEntity[]> {
  const url = toWsUrl(baseUrl);
  return new Promise<HaEntity[]>((resolve, reject) => {
    let socket: WebSocket;
    try {
      socket = new WebSocket(url);
    } catch {
      reject(new HaConnectError(`could not open a connection to ${url}; check the URL.`));
      return;
    }

    let settled = false;
    let authed = false;
    const reqId = 1;

    const timer = setTimeout(() => {
      finish(() =>
        reject(new HaConnectError('home assistant did not respond; check the URL and that the phone has network.')),
      );
    }, OPEN_TIMEOUT_MS);

    function finish(action: () => void): void {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      try {
        socket.close();
      } catch {
        // ignore: page is tearing the socket down anyway.
      }
      action();
    }

    socket.onerror = () => {
      finish(() => reject(new HaConnectError(`could not reach ${url}; check the URL and that the phone has network.`)));
    };

    socket.onclose = () => {
      finish(() => reject(new HaConnectError('home assistant closed the connection before it was ready.')));
    };

    socket.onmessage = ev => {
      let msg: { type: string; success?: boolean; result?: unknown };
      try {
        msg = JSON.parse(String(ev.data));
      } catch {
        return;
      }
      switch (msg.type) {
        case 'auth_required':
          socket.send(JSON.stringify({ type: 'auth', access_token: token }));
          return;
        case 'auth_ok':
          authed = true;
          socket.send(JSON.stringify({ id: reqId, type: 'get_states' }));
          return;
        case 'auth_invalid':
          finish(() => reject(new HaAuthError('home assistant rejected the token; re-check it.')));
          return;
        case 'result': {
          if (!authed) return;
          if (msg.success === false) {
            finish(() => reject(new HaConnectError('home assistant rejected the get_states request.')));
            return;
          }
          const raw = (msg.result ?? []) as Array<{
            entity_id: string;
            state: string;
            attributes: Record<string, unknown>;
          }>;
          const entities = raw.map(s => ({ entityId: s.entity_id, state: s.state, attributes: s.attributes ?? {} }));
          finish(() => resolve(entities));
          return;
        }
        default:
          return;
      }
    };
  });
}

function toWsUrl(baseUrl: string): string {
  const trimmed = baseUrl.trim().replace(/\/+$/, '');
  const withScheme = /^https?:\/\//i.test(trimmed) ? trimmed : `https://${trimmed}`;
  const ws = withScheme.replace(/^http/i, 'ws');
  return `${ws}/api/websocket`;
}
