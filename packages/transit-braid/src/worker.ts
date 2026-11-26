// Runs inside a Web Worker: loads the WebAssembly once, then answers requests (`serve.ts`).
// Heavy math happens here, so the page and map never freeze.

/// <reference lib="webworker" />

import init from '../wasm/transit_braid_wasm.js';
import { serve, webWorkerPort } from './serve.ts';

// Start loading right away. `new URL(..., import.meta.url)` lets every modern bundler
// find the .wasm file and serve it next to this worker.
serve(
  init({
    module_or_path: new URL('../wasm/transit_braid_wasm_bg.wasm', import.meta.url),
  }),
  webWorkerPort(),
);
