import { defineConfig } from 'tsdown';

export default defineConfig({
  entry: ['src/vite.ts', 'src/push.ts'],
  target: 'es2022',
  format: ['esm'],
  sourcemap: true,
  clean: true,
  dts: true,
  external: [
    '@msgpack/msgpack',
    '@tailwindcss/vite',
    '@vitejs/plugin-react',
    'babel-plugin-react-compiler',
    'tailwindcss',
    'vite',
  ],
});
