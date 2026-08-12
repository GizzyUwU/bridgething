/// <reference types="vite/client" />

const OFF_DEVICE_URL = 'ws://127.0.0.1:8891/';

export function daemonUrl(): string {
  const override = import.meta.env.VITE_BRIDGETHING_URL;
  if (override) return override;
  return typeof window !== 'undefined' ? `ws://${window.location.host}/` : OFF_DEVICE_URL;
}
