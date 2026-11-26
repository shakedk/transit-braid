// transit-braid/node's worker (`createBraidWorker()`, a worker_threads worker): loads the WASM
// from disk, then answers requests like the browser's worker (`serve.ts`).

import { parentPort } from 'node:worker_threads';

import { loadWasm } from './node-wasm.ts';
import { serve } from './serve.ts';

const port = parentPort;
// parentPort is null only when this file is run as a main script.
if (port) {
  serve(loadWasm(), {
    onRequest: (listener) => {
      port.on('message', listener);
    },
    answer: (response, transfer) => {
      port.postMessage(response, transfer);
    },
  });
}
