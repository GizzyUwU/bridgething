import { useEffect, useState } from 'react';
import { AppState, type AppStateStatus } from 'react-native';

export function isAppActive(state: AppStateStatus | null): boolean {
  return state !== 'background';
}

export function useAppActive(): boolean {
  const [active, setActive] = useState(() =>
    isAppActive(AppState.currentState),
  );

  useEffect(() => {
    const sub = AppState.addEventListener('change', next =>
      setActive(isAppActive(next)),
    );
    return () => sub.remove();
  }, []);

  return active;
}
