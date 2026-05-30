import type { BridgethingSpotifyAuthConfig } from '@bridgething/session-react-native';
import { sha256 } from 'js-sha256';
import { create } from 'zustand';

import {
  dismissVerificationBrowser,
  openVerificationBrowser,
} from './auth-browser';
import { getSession, useSessionStore } from './session';
import { getPreferredAuthMethod, setPreferredAuthMethod } from './storage';

export type SpotifyAuthMethod = 'deviceCode' | 'pkce';

export { setPreferredAuthMethod };

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

export async function availableAuthMethods(): Promise<SpotifyAuthMethod[]> {
  const c = await authConfig();
  const out: SpotifyAuthMethod[] = [];
  if (c.deviceCodePsk) out.push('deviceCode');
  if (c.pkceClientId) out.push('pkce');
  return out;
}

export async function effectiveAuthMethod(): Promise<SpotifyAuthMethod | null> {
  const avail = await availableAuthMethods();
  if (avail.length === 0) return null;
  const stored = getPreferredAuthMethod();
  if (stored && avail.includes(stored)) return stored;
  return avail.includes('deviceCode') ? 'deviceCode' : avail[0];
}

type Flow = { cancelled: boolean };
let activeFlow: Flow | null = null;

export function cancelSignIn(): void {
  if (activeFlow) activeFlow.cancelled = true;
  activeFlow = null;
  dismissVerificationBrowser().catch(() => {});
  usePkceWebView.getState().request?.onCancel();
  useSessionStore.getState().setAuthState({ kind: 'idle' });
}

export async function signIn(method?: SpotifyAuthMethod): Promise<void> {
  const chosen = method ?? (await effectiveAuthMethod());
  if (!chosen) throw new Error('no Spotify sign-in method configured');
  const c = await authConfig();

  const flow: Flow = { cancelled: false };
  activeFlow = flow;
  useSessionStore.getState().setAuthState({ kind: 'pending' });

  try {
    if (chosen === 'pkce') await signInPkce(c, flow);
    else await signInDeviceCode(c, flow);
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
    true,
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

async function signInDeviceCode(
  c: BridgethingSpotifyAuthConfig,
  flow: Flow,
): Promise<void> {
  const headers = {
    'Content-Type': 'application/x-www-form-urlencoded',
    Authorization: `Bearer ${c.deviceCodePsk}`,
  };

  const codeResp = await fetch(c.deviceCodeUrl, {
    method: 'POST',
    headers,
    body: form({
      scope: c.scopes.join(','),
      description: c.deviceCodeDescription,
    }),
  });
  if (!codeResp.ok) {
    throw new Error(`device code request failed (${codeResp.status})`);
  }
  const code = (await codeResp.json()) as {
    user_code: string;
    device_code: string;
    verification_url: string;
    verification_url_prefilled: string;
    interval?: number;
    expires_in?: number;
  };
  if (flow.cancelled) return;

  useSessionStore.getState().setAuthState({
    kind: 'pending',
    userCode: code.user_code,
    verificationUrl: code.verification_url,
    verificationUrlComplete: code.verification_url_prefilled,
  });
  openVerificationBrowser(code.verification_url_prefilled).catch(() => {});

  const deadline = Date.now() + (code.expires_in ?? 600) * 1000;
  let wait = Math.max(code.interval ?? 5, 1);

  while (Date.now() < deadline) {
    if (flow.cancelled) return;
    await sleep(wait * 1000);
    if (flow.cancelled) return;

    const tokenResp = await fetch(c.deviceCodeTokenUrl, {
      method: 'POST',
      headers,
      body: form({
        grant_type: 'urn:ietf:params:oauth:grant-type:device_code',
        device_code: code.device_code,
      }),
    });

    if (tokenResp.ok) {
      const tok = (await tokenResp.json()) as {
        access_token: string;
        refresh_token?: string;
      };
      dismissVerificationBrowser().catch(() => {});
      await getSession().completeSpotifySignIn(
        tok.access_token,
        tok.refresh_token ?? '',
        false,
      );
      return;
    }

    const body = (await tokenResp.json().catch(() => ({}))) as {
      error?: string;
    };
    if (body.error === 'authorization_pending') continue;
    if (body.error === 'slow_down') {
      wait += 5;
      continue;
    }
    throw new Error(body.error ?? `token request failed (${tokenResp.status})`);
  }
  throw new Error('the code expired before sign-in completed');
}

function form(params: Record<string, string>): string {
  return Object.entries(params)
    .map(([k, v]) => `${encodeURIComponent(k)}=${encodeURIComponent(v)}`)
    .join('&');
}

function sleep(ms: number): Promise<void> {
  return new Promise(resolve => setTimeout(resolve, ms));
}
