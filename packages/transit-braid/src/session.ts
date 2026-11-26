// Sessions: `sideBySide()` with a memory. A session keeps the engine's cache (the line graph and
// the order of its last routes) between its own calls, so a zoom change reruns geometry only and
// an edit rebuilds incrementally and keeps the order it had (warm start). Plain `sideBySide()`
// calls reuse a cache only for exactly the last plain call's routes, stations and cap (a zoom
// change); anything else starts from an empty one, so they depend on nothing before them.

import { closed } from './errors.ts';
import type { SideBySideOptions } from './options.ts';
import { RouteKeys, type RoutesInput, type SideBySideResult } from './routes.ts';
import { sideBySideOn, type SessionHost } from './side-by-side-on.ts';

/**
 * `sideBySide()` for an editor or a map that redraws: each call starts from this session's last
 * call (the same routes at another zoom only redraw; an edit keeps the rest of the order stable).
 * Calls run one after another, in the order made. `close()` it when done, to free its memory.
 */
export interface BraidSession {
  /**
   * As `sideBySide()`: the same input, output and errors, using and updating this session's
   * memory. Rejects with a `BraidError` whose code is `CLOSED` once the session is closed.
   */
  sideBySide(routes: RoutesInput, options: SideBySideOptions): Promise<SideBySideResult>;
  /**
   * Frees the session's memory. Calls made before still finish; later calls reject with code
   * `CLOSED`. Closing again does nothing.
   */
  close(): void;
}

/** Session ids, unique in this copy of the package (every worker it starts sees them). */
let nextSession = 1;

/**
 * A session on the worker `host()` gives at each call: a fixed one (your own worker), or the
 * shared one, which may be a new worker after the last one failed (the session then starts over
 * there with an empty memory).
 */
export function openSession(host: () => SessionHost): BraidSession {
  const id = nextSession++;
  const keys = new RouteKeys();
  let isClosed = false;
  /** The worker the last call went to: the one holding this session's memory. */
  let last: SessionHost | undefined;
  const onHost = (): SessionHost => {
    last = host();
    return last;
  };
  return {
    async sideBySide(routes, options) {
      if (isClosed) throw closed();
      return sideBySideOn(onHost, routes, options, { id, keys });
    },
    close() {
      if (isClosed) return;
      isClosed = true;
      last?.closeSession(id);
    },
  };
}
