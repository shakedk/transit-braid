// `transit-braid/inline`: the same API as `transit-braid`, for bundlers that can't follow
// `new URL(..., import.meta.url)`. The worker and its WASM are inside this one script; the worker
// starts from a Blob URL. Built into one file, `dist/inline.js`, by this package's build
// (`vite.config.ts`).

import { type BraidWorker, connect, sharedWorker } from './connection.ts';
// oxlint-disable-next-line import/default -- Vite's `?worker&inline` gives the default export (inline-env.d.ts)
import InlineWorker from './inline-worker.ts?worker&inline';
import type { SideBySideOptions } from './options.ts';
import type { RoutesInput, SideBySideResult } from './routes.ts';
import { type BraidSession, openSession } from './session.ts';
import { sideBySideOn } from './side-by-side-on.ts';
import { webPort } from './web-port.ts';

export * from './public.ts';

/** One worker for every call, started on the first one, and again after it failed. */
const shared = sharedWorker(() => connect(webPort(new InlineWorker())));

/** Starts a Braid worker from this script (no separate worker or .wasm file). */
export function createBraidWorker(): BraidWorker {
  return connect(webPort(new InlineWorker()));
}

/** As `transit-braid`'s `sideBySide()`; the worker comes from this script. */
export async function sideBySide(
  routes: RoutesInput,
  options: SideBySideOptions,
): Promise<SideBySideResult> {
  return sideBySideOn(shared, routes, options);
}

/** As `transit-braid`'s `createSession()`; on this script's shared worker. */
export function createSession(): BraidSession {
  return openSession(shared);
}
