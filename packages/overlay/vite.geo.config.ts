import { defineConfig } from 'vite';

export default defineConfig({
  build: {
    lib: {
      entry: 'src/geo.ts',
      name: 'BridgethingGeo',
      formats: ['iife'],
      fileName: () => 'geo.js',
    },
    outDir: 'dist',
    minify: true,
    emptyOutDir: false,
    cssCodeSplit: false,
  },
});
