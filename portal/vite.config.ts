import { svelte } from '@sveltejs/vite-plugin-svelte';
import { defineConfig } from 'vite';

// `npm run dev` proxies the API to a display on BOARD (its address on the network).
const board = process.env.BOARD ?? 'http://coverussi.local';

export default defineConfig({
  plugins: [svelte()],
  server: {
    port: Number(process.env.PORT) || 5173,
    strictPort: Boolean(process.env.PORT),
    proxy: { '/api': { target: board, ws: true, changeOrigin: true } }
  },
  // The firmware embeds dist-board/ (firmware-rs/build.rs): one script, one stylesheet.
  build: { outDir: 'dist-board', emptyOutDir: true, assetsInlineLimit: 8192, cssCodeSplit: false, modulePreload: false }
});
