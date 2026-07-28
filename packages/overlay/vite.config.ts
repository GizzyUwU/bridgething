import { defineConfig } from 'vite';

export default defineConfig({
  build: {
    lib: {
      entry: 'src/index.ts',
      name: 'BridgethingOverlay',
      formats: ['iife'],
      fileName: () => 'overlay.js',
    },
    outDir: 'dist',
    minify: true,
    emptyOutDir: true,
    cssCodeSplit: false,
  },
});
