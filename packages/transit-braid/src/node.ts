// `transit-braid/node`: the same `sideBySide()` for Node 24+, on the caller's own thread.
// The WASM is read from disk and compiled on the first call, then called directly: no worker, so
// a call blocks this thread while it runs (up to about a second for a big city's first draw).
// An app whose thread must not block starts its own worker: `createBraidWorker()`.

import { type BraidWorker, connect } from './connection.ts';
import { loadWasm } from './node-wasm.ts';
import { nodePort, startThread } from './node-port.ts';
import type { SideBySideOptions } from './options.ts';
import type { RoutesInput, SideBySideResult } from './routes.ts';
import { closeSession, run } from './run.ts';
import { type BraidSession, openSession } from './session.ts';
import { type SessionHost, sideBySideOn } from './side-by-side-on.ts';

export * from './public.ts';

/** The WASM loading (or loaded); a failed load is tried again on the next call. */
let loading: Promise<void> | undefined;

function load(): Promise<void> {
  // Only a missing or unreadable .wasm fails here.
  loading ??= loadWasm().catch((error: unknown) => {
    loading = undefined;
    throw error;
  });
  return loading;
}

/**
 * The pipeline on this thread, answering as the worker would. One WASM instance per process; after
 * a crash `run()` swaps it for a fresh one, so a crash fails only its own call. Calls and closes
 * wait for the WASM in the order made, so a session's close comes after its calls.
 */
const sameThread: SessionHost = {
  async runPipeline(request) {
    await load();
    return run({ id: 0, type: 'sideBySide', ...request });
  },
  closeSession(id) {
    // Nothing to free while the WASM never loaded: no call has run.
    load().then(
      () => {
        closeSession(id);
      },
      () => undefined,
    );
  },
};

/**
 * As `transit-braid`'s `sideBySide()`, in Node 24+: same input, same GeoJSON out, same errors.
 * Runs on this thread (no worker); the WASM is loaded from disk on the first call.
 */
export async function sideBySide(
  routes: RoutesInput,
  options: SideBySideOptions,
): Promise<SideBySideResult> {
  return sideBySideOn(() => sameThread, routes, options);
}

/**
 * As `transit-braid`'s `createSession()`: `sideBySide()` that remembers its last call, on this
 * thread. `close()` it when done.
 */
export function createSession(): BraidSession {
  return openSession(() => sameThread);
}

/**
 * Starts a Braid worker on a worker_threads thread: `sideBySide()` and its sessions run there,
 * so this thread never blocks. The worker keeps the process alive only while calls wait;
 * `terminate()` it when done.
 */
export function createBraidWorker(): BraidWorker {
  return connect(nodePort(startThread()));
}
