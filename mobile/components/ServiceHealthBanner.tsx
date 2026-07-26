import { Text, View } from 'react-native';

import { useSession } from '../lib/session';

export function ServiceHealthBanner() {
  const providers = useSession(s => s.providers);
  const degraded = providers.find(
    p => p.connected && p.serviceHealth.kind !== 'ok',
  );
  if (!degraded) return null;
  const health = degraded.serviceHealth;

  const message =
    health.kind === 'rateLimited'
      ? `${degraded.displayName} is rate-limiting requests${
          health.retryAfterSeconds
            ? ` - retrying in about ${Math.ceil(health.retryAfterSeconds)}s`
            : ' - retrying shortly'
        }.`
      : `${degraded.displayName} is unreachable right now - retrying.`;

  return (
    <View
      className="mb-3 rounded-xl px-3 py-2.5"
      style={{
        backgroundColor: 'rgba(217,119,6,0.12)',
        borderLeftWidth: 3,
        borderLeftColor: '#d97706',
      }}
    >
      <Text
        className="text-[12px] font-medium leading-[16px]"
        style={{ color: '#b45309' }}
      >
        {message}
      </Text>
    </View>
  );
}
