// The normal entry's worker: the script and its .wasm are separate files, found by the bundler
// through `new URL(..., import.meta.url)`. The inline entry starts its worker in `inline.ts`.
// The URL names `worker.js`, the built file in `dist/`. In this repo Vite serves `worker.ts` for
// that URL: like tsc, Vite resolves a `.js` path from a TypeScript file to its `.ts` source.

import { type BraidWorker, connect, type PipelineWorker } from './connection.ts';
import { webPort } from './web-port.ts';

export type { BraidWorker, PipelineWorker, SideBySideRequest } from './connection.ts';

/** Starts a Braid worker in the browser: `sideBySide()` on a worker of your own. */
export function createBraidWorker(): BraidWorker {
  return startWorker();
}

/** Starts a Braid worker with the internal pipeline call too (used by `sideBySide()`). */
export function startWorker(): PipelineWorker {
  return connect(webPort(new Worker(new URL('./worker.js', import.meta.url), { type: 'module' })));
}
