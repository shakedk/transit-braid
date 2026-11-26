// Builds `transit-braid/inline` into one self-contained file, dist/inline.js: the worker's script
// and the WASM (base64) are inside it. Run by this package's `build` script, after tsc has written
// the rest of dist/ (tsc's inline.js is replaced; its inline.d.ts is kept).
import { join } from 'node:path';

import { defineConfig, type UserConfig } from 'vite';

const config: UserConfig = defineConfig({
  root: import.meta.dirname,
  logLevel: 'warn',
  // So `?inline` turns the .wasm into a base64 data URL (Vite skips .wasm assets by default).
  assetsInclude: ['**/*.wasm'],
  build: {
    lib: {
      entry: join(import.meta.dirname, 'src', 'inline.ts'),
      formats: ['es'],
      fileName: () => 'inline.js',
    },
    outDir: join(import.meta.dirname, 'dist'),
    // tsc wrote the other entries and the types there first.
    emptyOutDir: false,
    target: 'es2022',
    minify: false,
  },
  worker: { format: 'iife' },
});

export default config;
