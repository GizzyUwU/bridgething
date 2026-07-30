import { useEffect, useState } from 'react';
import { AppState, type AppStateStatus } from 'react-native';

function isActive(state: AppStateStatus | null): boolean {
  return state !== 'background';
}

export function useAppActive(): boolean {
  const [active, setActive] = useState(() => isActive(AppState.currentState));

  useEffect(() => {
    const sub = AppState.addEventListener('change', next =>
      setActive(isActive(next)),
    );
    return () => sub.remove();
  }, []);

  return active;
}
