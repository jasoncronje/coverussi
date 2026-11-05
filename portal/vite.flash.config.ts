import { svelte } from '@sveltejs/vite-plugin-svelte';
import { defineConfig } from 'vite';

// The browser flasher, a static site for GitHub Pages: `npm run build:flash` writes dist-flash/,
// and CI adds the firmware images under dist-flash/firmware/.
export default defineConfig({
  root: 'flash',
  base: './',
  publicDir: '../public',
  plugins: [svelte()],
  server: { port: Number(process.env.PORT) || 5174, strictPort: Boolean(process.env.PORT) },
  build: { outDir: '../dist-flash', emptyOutDir: true }
});
