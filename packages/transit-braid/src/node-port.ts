// A worker_threads worker as a `WorkerPort` (`connection.ts`), for transit-braid/node's
// createBraidWorker(). Its own module so a test can hold the thread and stop it from outside.

import { Worker } from 'node:worker_threads';

import type { WorkerPort } from './connection.ts';

/**
 * The thread's script, next to this module: `node-worker.ts` when Node runs this package's
 * TypeScript source (in this repo), `node-worker.js` in the built package (`dist/`).
 */
// The built package's branch is tested by the pack test (`pnpm test:pack`).
const SCRIPT = import.meta.url.endsWith('.ts') ? './node-worker.ts' : './node-worker.js';

/**
 * Starts a thread running `node-worker.ts`. It gets none of this process's Node flags: it needs
 * none, and Node refuses a worker given a V8 flag (`--max-old-space-size`, which apply to every
 * thread anyway) or `--input-type` (only valid with `-e`); Node's own default passes the flags on.
 */
export function startThread(): Worker {
  return new Worker(new URL(SCRIPT, import.meta.url), { execArgv: [] });
}

/** A worker_threads worker as a `WorkerPort`; it keeps the process alive only while calls wait. */
export function nodePort(worker: Worker): WorkerPort {
  worker.unref();
  return {
    send: (request) => {
      // worker_threads' postMessage has no targetOrigin (that's only for window.postMessage).
      // oxlint-disable-next-line unicorn/require-post-message-target-origin
      worker.postMessage(request);
    },
    onAnswer: (listener) => {
      worker.on('message', listener);
    },
    onFailure: (listener) => {
      // An error thrown in the thread outside a call (only a broken build does that); the exit
      // below follows it.
      worker.on('error', (error: unknown) => {
        listener(error instanceof Error ? error.message : String(error));
      });
      worker.on('exit', (code) => {
        listener(`its thread stopped with exit code ${String(code)}`);
      });
      // Fires only if an answer can't be read (out of memory).
      worker.on('messageerror', () => listener('an answer could not be read'));
    },
    // Tested by a plain node process exiting (node-entry.test.ts).
    busy: (waiting) => {
      if (waiting) worker.ref();
      else worker.unref();
    },
    terminate: () => {
      void worker.terminate();
    },
  };
}
