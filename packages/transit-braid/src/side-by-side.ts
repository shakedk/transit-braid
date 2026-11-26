// The library's main call: GeoJSON routes in, side-by-side GeoJSON lines out.

import { startWorker } from './client.ts';
import { sharedWorker } from './connection.ts';
import type { SideBySideOptions } from './options.ts';
import type { RoutesInput, SideBySideResult } from './routes.ts';
import { type BraidSession, openSession } from './session.ts';
import { sideBySideOn } from './side-by-side-on.ts';

/** One worker for every call, started on the first one, and again after it failed. */
const shared = sharedWorker(startWorker);

/**
 * Draws overlapping routes side by side. `routes` is a GeoJSON FeatureCollection of up to 70
 * LineStrings in `[lng, lat]`, each with a unique `id` (GeoJSON's own field). Resolves to one
 * line per route (same id, same direction, properties copied) plus, for the `stations` option,
 * one bar across the bundle per station near a route.
 *
 * Pure: the same input gives the same output, whatever was drawn before. Only a zoom change is
 * fast: when the routes, stations and lane cap are exactly the last call's, only the geometry is
 * drawn again (the same bits a first call draws); any other change draws from scratch. An editor
 * uses `createSession()`, which also rebuilds only what an edit changed and keeps the lanes the
 * routes had. The work runs in a Web Worker, started on the first call and shared by all calls and
 * sessions.
 * Rejects with an `InputError` saying which route, station or option is wrong and why (`code`,
 * plus `routeId`, `index`, `point`, `stationId` or `option` where they apply). A failure inside
 * transit-braid itself rejects with an `InternalError`; it recovers by itself, so the next call
 * works.
 */
export async function sideBySide(
  routes: RoutesInput,
  options: SideBySideOptions,
): Promise<SideBySideResult> {
  return sideBySideOn(shared, routes, options);
}

/**
 * Starts a session: `sideBySide()` that remembers its last call, for an editor or a map that
 * redraws. A zoom change then redraws only the geometry, and an edit rebuilds only what changed
 * and keeps the order it had (lines don't jump lanes). Runs on the shared worker; `close()` it
 * when done.
 */
export function createSession(): BraidSession {
  return openSession(shared);
}
