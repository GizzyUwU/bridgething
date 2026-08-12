import type { BridgethingAuthState } from '@bridgething/session-react-native';
import { useEffect, useRef } from 'react';
import { Linking, Text, View } from 'react-native';
import InAppBrowser from 'react-native-inappbrowser-reborn';

import { Button } from './Button';
import { Spinner } from './Spinner';
import { TEXT, TYPE } from '../lib/theme';

const CODE_TRACKING = TYPE.title * 0.2;

export function PendingAuth({
  state,
  onCancel,
  onRetry,
}: {
  state: BridgethingAuthState;
  onCancel?: () => void;
  onRetry?: () => void;
}) {
  const pendingUrl =
    state.kind === 'pending'
      ? (state.verificationUrlComplete ?? state.verificationUrl ?? null)
      : null;

  const openedFor = useRef<string | null>(null);
  useEffect(() => {
    if (!pendingUrl) {
      openedFor.current = null;
      return;
    }
    const key =
      state.kind === 'pending' ? (state.userCode ?? pendingUrl) : pendingUrl;
    if (openedFor.current === key) return;
    openedFor.current = key;
    void openAuthUrl(pendingUrl);
  }, [pendingUrl, state]);

  if (state.kind === 'pending') {
    return (
      <View className="border border-accent bg-accent-soft p-4">
        <View className="flex-row items-center gap-2">
          <Spinner tone="accent" />
          <Text
            className="font-mono uppercase text-accent"
            style={TEXT.eyebrow}
          >
            waiting on provider
          </Text>
        </View>
        {state.userCode ? (
          <View className="mt-3">
            <Text className="font-sans text-muted" style={TEXT.hint}>
              enter this code at spotify.com/pair
            </Text>
            <Text
              className="mt-1 font-mono text-fg"
              style={{ fontSize: TYPE.title, letterSpacing: CODE_TRACKING }}
              selectable
            >
              {state.userCode}
            </Text>
            <Text
              className="mt-1 font-mono uppercase text-dim"
              style={TEXT.eyebrow}
            >
              long press to copy
            </Text>
          </View>
        ) : null}
        {pendingUrl ? (
          <View className="mt-3 self-start">
            <Button
              variant="secondary"
              size="sm"
              full={false}
              onPress={() => void openAuthUrl(pendingUrl)}
            >
              open authorization
            </Button>
          </View>
        ) : null}
        {state.verificationUrl ? (
          <Text
            className="mt-2 font-mono text-dim"
            style={TEXT.hint}
            selectable
          >
            {state.verificationUrl}
          </Text>
        ) : null}
        {onCancel ? (
          <View className="mt-3 self-start">
            <Button variant="ghost" size="sm" full={false} onPress={onCancel}>
              cancel
            </Button>
          </View>
        ) : null}
      </View>
    );
  }
  if (state.kind === 'failed') {
    return (
      <View className="border border-err bg-err-soft p-4">
        <Text className="font-mono uppercase text-err" style={TEXT.eyebrow}>
          sign-in failed
        </Text>
        <Text className="mt-1 font-sans text-err" style={TEXT.body}>
          {state.message ?? 'unknown error'}
        </Text>
        {onRetry ? (
          <View className="mt-3 self-start">
            <Button
              variant="secondary"
              size="sm"
              full={false}
              onPress={onRetry}
            >
              try again
            </Button>
          </View>
        ) : null}
      </View>
    );
  }
  return null;
}

const SPOTIFY_SCHEME = 'spotify://';

async function openAuthUrl(url: string) {
  try {
    if (await Linking.canOpenURL(SPOTIFY_SCHEME)) {
      await Linking.openURL(url);
      return;
    }
  } catch {
    // fall through to the in-app browser path
  }

  try {
    if (await InAppBrowser.isAvailable()) {
      await InAppBrowser.open(url, {
        animated: true,
        modalEnabled: true,
        enableDefaultShare: false,
      });
      return;
    }
    await Linking.openURL(url);
  } catch {
    // the code + url are shown in the card as a manual fallback.
  }
}
