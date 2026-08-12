import NetInfo from '@react-native-community/netinfo';
import { create } from 'zustand';

type ReachabilityState = { reachable: boolean };

const useReachabilityStore = create<ReachabilityState>(() => ({
  reachable: true,
}));

let listening = false;

export function startReachability(): void {
  if (listening) return;
  listening = true;
  NetInfo.addEventListener(state => {
    useReachabilityStore.setState({
      reachable: state.isInternetReachable ?? state.isConnected ?? true,
    });
  });
}

export function useReachable(): boolean {
  return useReachabilityStore(s => s.reachable);
}
