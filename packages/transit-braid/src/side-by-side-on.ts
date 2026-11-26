// `sideBySide()`'s work, on whichever worker the entry starts (the normal one, the inline one, or
// Node's own thread), as a plain call or with a session's cache.

import { asBraidError } from './errors.ts';
import { resolveOptions, type SideBySideOptions } from './options.ts';
import type { PipelineAnswer, SideBySideCall } from './protocol.ts';
import {
  buildResult,
  flattenRoutes,
  RouteKeys,
  type RoutesInput,
  type SideBySideResult,
} from './routes.ts';
import { buildBars, flattenStations } from './stations.ts';

/** The worker's `sideBySide` request, without its id. */
export type SideBySideRequest = Omit<SideBySideCall, 'id' | 'type'>;

/** Where the pipeline runs: a worker, or Node's own thread. */
export interface SessionHost {
  /** Runs the whole pipeline; returns the lines flat, like the request. */
  runPipeline(request: SideBySideRequest): Promise<PipelineAnswer>;
  /** Frees session `id`'s memory, after the calls already made; never throws. */
  closeSession(id: number): void;
}

/** A session's part in a call: its id, and its numbering of the route ids. */
export interface SessionPart {
  id: number;
  keys: RouteKeys;
}

/**
 * `sideBySide()` on the worker `worker()` gives (called only when there is work: no routes, no
 * worker), as a plain call (pure), or with `session`'s cache.
 */
export async function sideBySideOn(
  worker: () => Pick<SessionHost, 'runPipeline'>,
  routes: RoutesInput,
  options: SideBySideOptions,
  session?: SessionPart,
): Promise<SideBySideResult> {
  const resolved = resolveOptions(options);
  const flat = flattenRoutes(routes);
  const stations = flattenStations(options.stations);
  if (flat.ids.length === 0) return buildResult(flat, new Float64Array(), new Uint32Array());

  const pipeline = worker();
  let answer: PipelineAnswer;
  try {
    answer = await pipeline.runPipeline({
      session: session?.id,
      coords: flat.coords,
      starts: flat.starts,
      routeKeys: (session?.keys ?? new RouteKeys()).keysFor(flat.ids),
      stations: stations.coords,
      stationGroups: stations.groups,
      zoom: resolved.scale.kind === 'zoom' ? resolved.scale.zoom : undefined,
      metersPerPixel:
        resolved.scale.kind === 'metersPerPixel' ? resolved.scale.metersPerPixel : undefined,
      lineWidth: resolved.lineWidth,
      spacing: resolved.spacing,
      ...(resolved.maxSideBySide === undefined ? {} : { maxSideBySide: resolved.maxSideBySide }),
    });
  } catch (error) {
    throw asBraidError(error, flat.ids, stations.ids);
  }
  return {
    ...buildResult(flat, answer.coords, answer.starts, answer.dropped),
    stations: buildBars(stations, answer.bars),
  };
}
