import type { BridgethingAuthState } from '@bridgething/session-react-native';
import { ActivityIndicator, Text, View } from 'react-native';

import { Button } from './Button';

/**
 * Renders the device-code prompt block when auth is pending. The
 * `verificationUrl` line is what users paste into a browser when the
 * SFSafariViewController didn't auto-open (or they dismissed it).
 *
 * The `failed` state shows the daemon-reported reason and a retry CTA.
 */
export function PendingAuth({
  state,
  onCancel,
  onRetry,
}: {
  state: BridgethingAuthState;
  onCancel?: () => void;
  onRetry?: () => void;
}) {
  if (state.kind === 'pending') {
    return (
      <View className="rounded-md bg-card p-4">
        <View className="flex-row items-center gap-2">
          <ActivityIndicator size="small" />
          <Text className="text-xs uppercase tracking-widest text-muted-foreground">
            waiting on auth
          </Text>
        </View>
        {state.userCode ? (
          <View className="mt-3">
            <Text className="text-xs text-muted-foreground">
              enter this code
            </Text>
            <Text className="mt-0.5 font-mono text-2xl font-semibold tracking-wider text-foreground">
              {state.userCode}
            </Text>
          </View>
        ) : null}
        {state.verificationUrl ? (
          <Text className="mt-2 text-xs text-muted-foreground">
            on {state.verificationUrl}
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
      <View className="rounded-md bg-destructive/10 p-4">
        <Text className="text-xs uppercase tracking-widest text-destructive">
          sign-in failed
        </Text>
        <Text className="mt-1 text-sm text-destructive">
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
