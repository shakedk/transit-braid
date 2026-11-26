import { defineConfig, type UserConfig, type UserConfigFnObject } from 'vite';

const dev: UserConfig = {
  // transit-braid is a workspace package with its own worker and .wasm;
  // let Vite serve it as source instead of pre-bundling it.
  optimizeDeps: { exclude: ['transit-braid', 'transit-braid-deck', 'maplibre-gl'] },
  server: { port: 5199, strictPort: true },
};

// `--mode site` (`pnpm build:site`): the demo as a static site, both pages, every URL relative so
// it works under any path (GitHub Pages serves it under /transit-braid/). Every import of
// transit-braid, the deck layer's too, gets transit-braid/inline: the worker and the WASM inside
// one file (its `dist/inline.js`, so run `pnpm build:packages` first).
const site: UserConfig = {
  base: './',
  resolve: { alias: [{ find: /^transit-braid$/, replacement: 'transit-braid/inline' }] },
  build: {
    outDir: 'dist-site',
    emptyOutDir: true,
    rollupOptions: { input: { main: 'index.html', deck: 'deck.html' } },
  },
  preview: { port: 5197, strictPort: true },
};

const config: UserConfigFnObject = defineConfig(({ mode }) => (mode === 'site' ? site : dev));

export default config;
