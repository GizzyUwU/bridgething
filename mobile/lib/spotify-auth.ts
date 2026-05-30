import type { BridgethingSpotifyAuthConfig } from '@bridgething/session-react-native';
import { authorize as appAuthAuthorize } from 'react-native-app-auth';

import {
  dismissVerificationBrowser,
  openVerificationBrowser,
} from './auth-browser';
import { getSession, useSessionStore } from './session';
import { getPreferredAuthMethod, setPreferredAuthMethod } from './storage';

export type SpotifyAuthMethod = 'deviceCode' | 'pkce';

export { setPreferredAuthMethod };

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
    if (chosen === 'pkce') await signInPkce(c);
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

async function signInPkce(c: BridgethingSpotifyAuthConfig): Promise<void> {
  const result = await appAuthAuthorize({
    clientId: c.pkceClientId,
    redirectUrl: c.pkceRedirectUri,
    scopes: c.scopes,
    serviceConfiguration: {
      authorizationEndpoint: c.pkceAuthorizeUrl,
      tokenEndpoint: c.pkceTokenUrl,
    },
    usePKCE: true,
  });
  await getSession().completeSpotifySignIn(
    result.accessToken,
    result.refreshToken ?? '',
    true,
  );
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
