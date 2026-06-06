import type { BridgethingSpotifyAuthConfig } from '@bridgething/session-react-native';
import { sha256 } from 'js-sha256';
import { create } from 'zustand';

import { getSession, useSessionStore } from './session';

export type PkceWebRequest = {
  url: string;
  redirectPrefix: string;
  onCallback: (callbackUrl: string) => void;
  onCancel: () => void;
};

export const usePkceWebView = create<{
  request: PkceWebRequest | null;
  setRequest: (request: PkceWebRequest | null) => void;
}>(set => ({
  request: null,
  setRequest: request => set({ request }),
}));

let configCache: BridgethingSpotifyAuthConfig | null = null;

async function authConfig(): Promise<BridgethingSpotifyAuthConfig> {
  if (!configCache) configCache = await getSession().spotifyAuthConfig();
  return configCache;
}

type Flow = { cancelled: boolean };
let activeFlow: Flow | null = null;

export function cancelSignIn(): void {
  if (activeFlow) activeFlow.cancelled = true;
  activeFlow = null;
  usePkceWebView.getState().request?.onCancel();
  useSessionStore.getState().setAuthState({ kind: 'idle' });
}

export async function signIn(): Promise<void> {
  const c = await authConfig();
  if (!c.pkceClientId) throw new Error('no Spotify sign-in method configured');

  const flow: Flow = { cancelled: false };
  activeFlow = flow;
  useSessionStore.getState().setAuthState({ kind: 'pending' });

  try {
    await signInPkce(c, flow);
  } catch (err) {
    if (flow.cancelled) {
      useSessionStore.getState().setAuthState({ kind: 'idle' });
    } else {
      useSessionStore.getState().setAuthState({
        kind: 'failed',
        message: err instanceof Error ? err.message : String(err),
      });
    }
    throw err;
  } finally {
    if (activeFlow === flow) activeFlow = null;
  }
}

async function signInPkce(
  c: BridgethingSpotifyAuthConfig,
  flow: Flow,
): Promise<void> {
  const verifier = randomUrlSafe(32);
  const codeChallenge = base64Url(new Uint8Array(sha256.arrayBuffer(verifier)));
  const state = randomUrlSafe(16);

  const authorizeUrl =
    `${c.pkceAuthorizeUrl}?` +
    form({
      client_id: c.pkceClientId,
      response_type: 'code',
      redirect_uri: c.pkceRedirectUri,
      scope: c.scopes.join(' '),
      state,
      code_challenge: codeChallenge,
      code_challenge_method: 'S256',
    });

  const callbackUrl = await new Promise<string>((resolve, reject) => {
    usePkceWebView.getState().setRequest({
      url: authorizeUrl,
      redirectPrefix: c.pkceRedirectUri,
      onCallback: url => {
        usePkceWebView.getState().setRequest(null);
        resolve(url);
      },
      onCancel: () => {
        usePkceWebView.getState().setRequest(null);
        reject(new Error('sign-in cancelled'));
      },
    });
  });
  if (flow.cancelled) return;

  const params = parseCallbackParams(callbackUrl);
  if (params.error) throw new Error(params.error);
  if (params.state !== state) throw new Error('auth state mismatch');
  if (!params.code) throw new Error('no authorization code in callback');

  const tokenResp = await fetch(c.pkceTokenUrl, {
    method: 'POST',
    headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
    body: form({
      client_id: c.pkceClientId,
      grant_type: 'authorization_code',
      code: params.code,
      redirect_uri: c.pkceRedirectUri,
      code_verifier: verifier,
    }),
  });
  if (!tokenResp.ok) {
    throw new Error(`token exchange failed (${tokenResp.status})`);
  }
  const tok = (await tokenResp.json()) as {
    access_token: string;
    refresh_token?: string;
  };
  await getSession().completeSpotifySignIn(
    tok.access_token,
    tok.refresh_token ?? '',
  );
}

function parseCallbackParams(url: string): {
  code?: string;
  state?: string;
  error?: string;
} {
  const query = url.split('?')[1]?.split('#')[0] ?? '';
  const out: Record<string, string> = {};
  for (const pair of query.split('&')) {
    if (!pair) continue;
    const [k, v = ''] = pair.split('=');
    out[decodeURIComponent(k)] = decodeURIComponent(v.replace(/\+/g, ' '));
  }
  return { code: out.code, state: out.state, error: out.error };
}

const SECURE_RANDOM = globalThis as unknown as {
  crypto: { getRandomValues<T extends ArrayBufferView>(array: T): T };
};

function randomUrlSafe(byteCount: number): string {
  const bytes = new Uint8Array(byteCount);
  SECURE_RANDOM.crypto.getRandomValues(bytes);
  return base64Url(bytes);
}

const B64 = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/';

function base64Url(bytes: Uint8Array): string {
  let out = '';
  for (let i = 0; i < bytes.length; i += 3) {
    const b0 = bytes[i];
    const b1 = i + 1 < bytes.length ? bytes[i + 1] : 0;
    const b2 = i + 2 < bytes.length ? bytes[i + 2] : 0;
    out += B64[b0 >> 2];
    out += B64[((b0 & 3) << 4) | (b1 >> 4)];
    out += i + 1 < bytes.length ? B64[((b1 & 15) << 2) | (b2 >> 6)] : '';
    out += i + 2 < bytes.length ? B64[b2 & 63] : '';
  }
  return out.replace(/\+/g, '-').replace(/\//g, '_');
}

function form(params: Record<string, string>): string {
  return Object.entries(params)
    .map(([k, v]) => `${encodeURIComponent(k)}=${encodeURIComponent(v)}`)
    .join('&');
}
