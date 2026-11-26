// A browser `Worker` as a `WorkerPort` (`connection.ts`), for the normal and inline entries. Its
// own module, like `node-port.ts`, so the types the Node entry ships never name the DOM's `Worker`
// (a Node project without the DOM types type-checks them).

import type { WorkerPort } from './connection.ts';
import type { WorkerResponse } from './protocol.ts';

/** A browser `Worker` as a `WorkerPort`. */
export function webPort(worker: Worker): WorkerPort {
  return {
    // Worker.postMessage has no targetOrigin (that's only for window.postMessage).
    // oxlint-disable-next-line unicorn/require-post-message-target-origin
    send: (request) => worker.postMessage(request),
    onAnswer: (listener) => {
      worker.addEventListener('message', (event: MessageEvent<WorkerResponse>) => {
        listener(event.data);
      });
    },
    onFailure: (listener) => {
      worker.addEventListener('error', (event: ErrorEvent) => {
        // A script that fails to load gives no message.
        listener(event.message === '' ? 'it failed to start' : event.message);
      });
      // Fires only if the browser can't read an answer (out of memory).
      worker.addEventListener('messageerror', () => listener('an answer could not be read'));
    },
    terminate: () => {
      worker.terminate();
    },
  };
}
