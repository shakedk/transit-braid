// Messages between the main thread and the Braid worker.
// Every request has an `id`; the worker answers with the same `id`, so many requests
// can be in flight at once and answers can be matched even if they arrive out of order.

import type { InternalErrorCode, Refusal } from './errors.ts';

/** Main thread → worker: one `sideBySide()` call (flat), or the end of a session. */
export type WorkerRequest = SideBySideCall | CloseSession;

/** One `sideBySide()` call, flat. */
export interface SideBySideCall {
  id: number;
  type: 'sideBySide';
  /**
   * The session whose cache the call uses and updates (the worker makes it on first use). Left
   * out: a plain call, whose answer depends on nothing before it (the worker reuses a cache only
   * for exactly the last plain call's routes, stations and cap, else starts from an empty one).
   */
  session?: number | undefined;
  /** Flat `[lng, lat, lng, lat, ...]`. */
  coords: Float64Array;
  /** Where each route starts in `coords`, counted in points. */
  starts: Uint32Array;
  /**
   * Each route's id as a number, the same number for the same id on every call, so
   * an edit is matched by id. Left out: matched by place in the list.
   */
  routeKeys?: Uint32Array | undefined;
  /** Stations, flat `[lng, lat, ...]`, one point each. */
  stations: Float64Array;
  /**
   * Each station's name as a number, so same-name stops share a bar: equal numbers,
   * equal names; 0, no name. Left out: no names.
   */
  stationGroups?: Uint32Array | undefined;
  /** Exactly one of `zoom` and `metersPerPixel` is set. */
  zoom: number | undefined;
  metersPerPixel: number | undefined;
  lineWidth: number;
  spacing: number;
  /** Lane cap: at most this many lines side by side, 1 to 2³² − 1. Left out: no cap. */
  maxSideBySide?: number | undefined;
}

/** A session's end: the worker frees its cache. Answered by nothing. */
export interface CloseSession {
  id: number;
  type: 'closeSession';
  session: number;
}

/** What a `sideBySide` request gives back, flat. */
export interface PipelineAnswer {
  /** Every line's points, flat: `[lng, lat, lng, lat, ...]`. */
  coords: Float64Array;
  /** Where each line starts in `coords`, counted in points. */
  starts: Uint32Array;
  /** Four numbers per station (a bar's two ends), NaN for none. */
  bars: Float64Array;
  /** The routes a lane cap left out, as input indices, ascending; empty without a cap. */
  dropped: Uint32Array;
}

/** Worker → main thread: the answer, the engine's refusal of the input (named on the main
 * thread, which knows the ids), or an internal error. */
export type WorkerResponse =
  | ({ id: number; ok: true } & PipelineAnswer)
  | { id: number; ok: false; refusal: Refusal }
  | { id: number; ok: false; internal: { code: InternalErrorCode; message: string } };
