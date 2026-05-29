import { Text, View } from 'react-native';

import { useSession } from '../lib/session';

/** Shown while the active provider is signed in but its API is degraded
 *  (rate-limited or unreachable). The provider still reads as connected;
 *  this just makes the degraded state visible instead of silently failing. */
export function ServiceHealthBanner() {
  const health = useSession(s => s.serviceHealth);
  if (health.kind === 'ok') return null;

  const message =
    health.kind === 'rateLimited'
      ? `Spotify is rate-limiting requests${
          health.retryAfterSeconds
            ? ` - retrying in about ${Math.ceil(health.retryAfterSeconds)}s`
            : ' - retrying shortly'
        }.`
      : 'Spotify is unreachable right now - retrying.';

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
