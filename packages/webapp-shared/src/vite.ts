import tailwindcss from '@tailwindcss/vite';
import react from '@vitejs/plugin-react';
import { defineConfig, type UserConfig } from 'vite';

export type BridgethingViteOverrides = {
  plugins?: NonNullable<UserConfig['plugins']>;
  build?: UserConfig['build'];
  server?: UserConfig['server'];
};

export function defineBridgethingConfig(overrides: BridgethingViteOverrides = {}): UserConfig {
  return defineConfig({
    plugins: [react(), tailwindcss(), ...(overrides.plugins ?? [])],
    build: {
      target: 'es2022',
      sourcemap: true,
      ...(overrides.build ?? {}),
    },
    server: {
      host: true,
      ...(overrides.server ?? {}),
    },
  }) as UserConfig;
}
