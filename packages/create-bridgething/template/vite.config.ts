import tailwindcss from '@tailwindcss/vite';
import react from '@vitejs/plugin-react';
import { defineConfig } from 'vite';

// Car Thing browser is chromium 147 ARM at 800x480. Targeting `es2022`
// keeps bundles modern without polluting the bundle with downlevel helpers.
export default defineConfig({
  plugins: [react(), tailwindcss()],
  build: {
    target: 'es2022',
    sourcemap: true,
  },
  server: {
    host: true,
  },
});
