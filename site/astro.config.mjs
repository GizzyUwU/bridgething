import preact from '@astrojs/preact';
import sitemap from '@astrojs/sitemap';
import tailwindcss from '@tailwindcss/vite';
import { defineConfig } from 'astro/config';
import { fileURLToPath } from 'node:url';
import { APP_DETAIL_SHELL } from './src/lib/app-routes.ts';
import { FEATURES } from './src/lib/features.ts';

const EXCLUDED = ['/admin/sources', APP_DETAIL_SHELL, ...(FEATURES.browserFlasher ? [] : ['/install/flash'])];

export default defineConfig({
  site: 'https://bridgething.com',
  output: 'static',
  integrations: [
    preact(),
    sitemap({
      filter: page => {
        const path = new URL(page).pathname.replace(/\/$/, '');
        return !EXCLUDED.some(excluded => path === excluded.replace(/\/$/, ''));
      },
    }),
  ],
  trailingSlash: 'ignore',
  redirects: { '/apps/store': '/apps' },
  build: {
    format: 'directory',
    inlineStylesheets: 'auto',
  },
  vite: {
    plugins: [tailwindcss()],
    resolve: { alias: { 'node:zlib': fileURLToPath(new URL('./src/lib/zlib-shim.ts', import.meta.url)) } },
    server: { fs: { allow: ['..'] } },
  },
});
