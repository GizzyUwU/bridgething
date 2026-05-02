import { defineConfig } from 'tsdown';

export default defineConfig({
  entry: ['src/index.ts'],
  format: ['esm'],
  outExtensions: () => ({ js: '.mjs' }),
  outDir: 'dist',
  target: 'node20',
  shims: true,
  clean: true,
});
