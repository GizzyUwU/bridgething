import type { BridgethingAuthState } from '@bridgething/session-react-native';
import { useEffect, useRef } from 'react';
import { ActivityIndicator, Linking, Text, View } from 'react-native';
import InAppBrowser from 'react-native-inappbrowser-reborn';

import { Button } from './Button';

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
      <View
        className="overflow-hidden rounded-2xl border border-primary/30 bg-primary-soft p-4"
        style={{
          shadowColor: 'hsl(199 100% 44%)',
          shadowOpacity: 0.1,
          shadowRadius: 12,
          shadowOffset: { width: 0, height: 6 },
        }}
      >
        <View className="flex-row items-center gap-2">
          <ActivityIndicator size="small" color="hsl(199 100% 44%)" />
          <Text className="text-[11px] font-bold uppercase tracking-[0.18em] text-primary">
            waiting on provider
          </Text>
        </View>
        {state.userCode ? (
          <View className="mt-3">
            <Text className="text-[12px] text-muted-foreground">
              enter this code in your browser
            </Text>
            <Text
              className="mt-1 font-mono text-[28px] font-semibold tracking-[0.2em] text-foreground"
              selectable
            >
              {state.userCode}
            </Text>
          </View>
        ) : null}
        {pendingUrl ? (
          <View className="mt-3 self-start">
            <Button
              variant="secondary"
              size="sm"
              onPress={() => void openAuthUrl(pendingUrl)}
            >
              open authorization
            </Button>
          </View>
        ) : null}
        {state.verificationUrl ? (
          <Text className="mt-2 text-[12px] text-muted-foreground" selectable>
            {state.verificationUrl}
          </Text>
        ) : null}
        {onCancel ? (
          <View className="mt-3 self-start">
            <Button variant="ghost" size="sm" onPress={onCancel}>
              cancel
            </Button>
          </View>
        ) : null}
      </View>
    );
  }
  if (state.kind === 'failed') {
    return (
      <View className="rounded-2xl border border-destructive/30 bg-destructive-soft p-4">
        <Text className="text-[11px] font-bold uppercase tracking-[0.18em] text-destructive">
          sign-in failed
        </Text>
        <Text className="mt-1 text-[14px] text-destructive">
          {state.message ?? 'unknown error'}
        </Text>
        {onRetry ? (
          <View className="mt-3 self-start">
            <Button variant="secondary" size="sm" onPress={onRetry}>
              try again
            </Button>
          </View>
        ) : null}
      </View>
    );
  }
  return null;
}

async function openAuthUrl(url: string) {
  try {
    if (await InAppBrowser.isAvailable()) {
      await InAppBrowser.open(url, {
        animated: true,
        modalEnabled: true,
        enableDefaultShare: false,
      });
    } else {
      await Linking.openURL(url);
    }
  } catch {
    // the code + url are shown in the card as a manual fallback.
  }
}
