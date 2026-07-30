import { defineConfig } from 'vite';

// separate bundle from the overlay on purpose: a webapp may designate its own overlay body, and that
// substitution must not be able to take navigator.geolocation with it.
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
