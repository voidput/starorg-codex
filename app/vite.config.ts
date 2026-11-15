import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import tailwindcss from '@tailwindcss/vite';
import { fileURLToPath } from 'node:url';

// Tauri expects a fixed dev port and no clearing of its own output (tauri.conf.json devUrl).
export default defineConfig({
  plugins: [tailwindcss(), svelte()],
  resolve: {
    alias: { '@': fileURLToPath(new URL('./src', import.meta.url)) },
  },
  clearScreen: false,
  server: { port: 1420, strictPort: true, host: '127.0.0.1' },
  envPrefix: ['VITE_', 'TAURI_ENV_'],
  build: {
    target: 'es2022',
    // Fonts are self-hosted and bundled; never inline them as data: URIs (CSP font-src 'self').
    assetsInlineLimit: 0,
  },
});
