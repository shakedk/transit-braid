// transit-braid's errors: an `InputError` when the input can't be drawn (fix the input), an
// `InternalError` when transit-braid itself failed (never the input's fault), and a plain
// `BraidError` for a call on a worker you terminated or a session you closed. Every error has a
// stable `code`.
//
// `instanceof` works across copies of the package: the inline entry (`transit-braid/inline`) is
// one file with its own copy of these classes, and an app may load two copies (the inline entry
// next to the main one, or two versions). Each error carries its kinds under a symbol from the
// global registry, which every copy shares, and each class's `instanceof` reads them instead of
// the prototype chain.

import type { RouteId } from './routes.ts';

/**
 * Why the input was refused. Stable: compare these, not the message.
 *
 * - `BAD_OPTION`: an option is missing, of the wrong type or out of range (`option` names it).
 * - `BAD_ROUTES`: `routes` is not a FeatureCollection of LineString features (`index` and
 *   `routeId` name the feature, when one is at fault).
 * - `MISSING_ID`: a route has no usable GeoJSON `id` (`index`).
 * - `DUPLICATE_ID`: two routes share an id (`routeId`, and `index` of the second).
 * - `TOO_MANY_ROUTES`: more than 70 routes.
 * - `BAD_COORDINATE`: a point is not `[longitude, latitude]` numbers on the map (`routeId`,
 *   `index`, `point`).
 * - `TOO_FEW_POINTS`: a route has fewer than two distinct points (`routeId`, `index`).
 * - `TOO_MANY_POINTS`: more than 1,000,000 points, all routes together (`routeId` and `index`
 *   of the route that passes the limit).
 * - `CROSSES_ANTIMERIDIAN`: a route jumps more than 180° of longitude between two points
 *   (`routeId`, `index`, and `point` after the jump).
 * - `TOO_WIDE`: the routes spread more than 200 km east-west or north-south.
 * - `TOO_LONG`: the routes are longer than 2,000 km together.
 * - `TOO_COMPLEX`: valid input, but too much work to draw (points piled up, very many short
 *   crossing pieces, or stations × streets).
 * - `BAD_STATION`: `stations` is not a FeatureCollection of Points, or a station is not on the
 *   map (`index`, and `stationId` when it has one).
 * - `TOO_MANY_STATIONS`: more than 5,000 stations.
 */
export type InputErrorCode =
  | 'BAD_OPTION'
  | 'BAD_ROUTES'
  | 'MISSING_ID'
  | 'DUPLICATE_ID'
  | 'TOO_MANY_ROUTES'
  | 'BAD_COORDINATE'
  | 'TOO_FEW_POINTS'
  | 'TOO_MANY_POINTS'
  | 'CROSSES_ANTIMERIDIAN'
  | 'TOO_WIDE'
  | 'TOO_LONG'
  | 'TOO_COMPLEX'
  | 'BAD_STATION'
  | 'TOO_MANY_STATIONS';

/**
 * Why transit-braid itself failed. Stable: compare these, not the message.
 *
 * - `ENGINE_CRASHED`: the WebAssembly engine crashed (a bug, or no memory left). It restarted
 *   at once, so the next call works and starts fresh.
 * - `WORKER_FAILED`: the worker failed (its script, its thread, or an answer) and was stopped.
 *   `sideBySide()` starts a new one on the next call; start a new `createBraidWorker()` yourself.
 * - `INTERNAL`: anything else that should never happen, such as an answer that doesn't fit.
 */
export type InternalErrorCode = 'ENGINE_CRASHED' | 'WORKER_FAILED' | 'INTERNAL';

/** Every error's `code`: an input error, an internal one, `TERMINATED` (a call on a worker you
 * terminated) or `CLOSED` (a call on a session you closed). */
export type ErrorCode = InputErrorCode | InternalErrorCode | 'TERMINATED' | 'CLOSED';

/** What an `InputError` is about; only the fields that apply are set. */
export interface InputErrorDetails {
  /** The route at fault, by its GeoJSON `id`. */
  routeId?: RouteId;
  /** The station at fault, by its GeoJSON `id` (when it has one). */
  stationId?: RouteId;
  /** Where the route or station at fault is in its `features` list. */
  index?: number;
  /** The point at fault, by its place in the route's `coordinates`. */
  point?: number;
  /** The option at fault: `options`, `zoom`, `metersPerPixel`, `lineWidth`, `spacing` or
   * `maxSideBySide`. */
  option?: string;
}

/** The kinds an error is (`['BraidError', 'InputError']`), under a shared symbol. */
const KINDS = Symbol.for('transit-braid.error-kinds');

/** Adds `kind` to the kinds `error` carries. */
function mark(error: Error, kind: string): void {
  const kinds: unknown = Reflect.get(error, KINDS);
  Object.defineProperty(error, KINDS, {
    value: [...(Array.isArray(kinds) ? kinds : []), kind],
    configurable: true,
  });
}

/** Whether `value` is an error some copy of transit-braid made as a `kind`. */
function isKind(value: unknown, kind: string): boolean {
  if (typeof value !== 'object' || value === null) return false;
  const kinds: unknown = Reflect.get(value, KINDS);
  return Array.isArray(kinds) && kinds.includes(kind);
}

/**
 * Every error transit-braid rejects with: an `InputError`, an `InternalError`, or a plain
 * `BraidError` with code `TERMINATED` (a call on a worker you terminated) or `CLOSED` (a call on a
 * session you closed).
 */
export class BraidError extends Error {
  override name = 'BraidError';
  /** Why, as a stable code. */
  readonly code: ErrorCode;

  constructor(code: ErrorCode, message: string) {
    super(message);
    this.code = code;
    mark(this, 'BraidError');
  }

  /** True for an error from any copy of transit-braid. */
  static override [Symbol.hasInstance](value: unknown): value is BraidError {
    return isKind(value, 'BraidError');
  }
}

/**
 * The input can't be drawn: a route, station or option is wrong, or the whole input is too big.
 * The message says which and why; `code` says why as a stable code, and the details (`routeId`,
 * `index`, `point`, `stationId`, `option`) which, when they apply.
 */
export class InputError extends BraidError {
  override name = 'InputError';
  declare readonly code: InputErrorCode;
  /** The route at fault, by its GeoJSON `id`. */
  declare readonly routeId?: RouteId;
  /** The station at fault, by its GeoJSON `id` (when it has one). */
  declare readonly stationId?: RouteId;
  /** Where the route or station at fault is in its `features` list. */
  declare readonly index?: number;
  /** The point at fault, by its place in the route's `coordinates`. */
  declare readonly point?: number;
  /** The option at fault. */
  declare readonly option?: string;

  constructor(
    code: InputErrorCode,
    message: string,
    details: { [K in keyof InputErrorDetails]?: InputErrorDetails[K] | undefined } = {},
  ) {
    super(code, message);
    mark(this, 'InputError');
    for (const [key, value] of Object.entries(details)) {
      if (value !== undefined) Object.defineProperty(this, key, { value, enumerable: true });
    }
  }

  /** True for an `InputError` from any copy of transit-braid. */
  static override [Symbol.hasInstance](value: unknown): value is InputError {
    return isKind(value, 'InputError');
  }
}

/**
 * transit-braid itself failed, never the input's fault: the engine crashed, or a worker failed.
 * The message starts "Internal error in transit-braid, not a problem with your input".
 */
export class InternalError extends BraidError {
  override name = 'InternalError';
  declare readonly code: InternalErrorCode;

  constructor(code: InternalErrorCode, message: string) {
    super(code, message);
    mark(this, 'InternalError');
  }

  /** True for an `InternalError` from any copy of transit-braid. */
  static override [Symbol.hasInstance](value: unknown): value is InternalError {
    return isKind(value, 'InternalError');
  }
}

/** An `InternalError` whose message says it isn't the input's fault, then `what`. */
export function internalError(code: InternalErrorCode, what: string): InternalError {
  return new InternalError(
    code,
    `Internal error in transit-braid, not a problem with your input: ${what}`,
  );
}

/** The error for a call on a worker its owner terminated. */
export function terminated(): BraidError {
  return new BraidError('TERMINATED', 'Braid worker was terminated.');
}

/** The error for a call on a session its owner closed. */
export function closed(): BraidError {
  return new BraidError('CLOSED', 'Braid session was closed.');
}

/**
 * The engine's refusal of a call, as data: its code (an `InputErrorCode`, or `BAD_LAYOUT` for a
 * call whose flat layout the engine can't read), the reason without the subject, and the indices
 * of the route, point or station at fault. Crosses from the worker as a plain object.
 * Internal.
 */
export interface Refusal {
  code: string;
  reason: string;
  route?: number | undefined;
  point?: number | undefined;
  station?: number | undefined;
}

/** Thrown inside the package when the engine refused a call; `sideBySide()` turns it into an
 * `InputError` naming the route or station. Internal. */
export class Refused extends Error {
  override name = 'Refused';
  readonly refusal: Refusal;

  constructor(refusal: Refusal) {
    super(refusal.reason);
    this.refusal = refusal;
  }
}

/** The input error codes, to tell the engine's refusals of the input from its other refusals. */
const INPUT_CODES: ReadonlySet<string> = new Set<InputErrorCode>([
  'BAD_OPTION',
  'BAD_ROUTES',
  'MISSING_ID',
  'DUPLICATE_ID',
  'TOO_MANY_ROUTES',
  'BAD_COORDINATE',
  'TOO_FEW_POINTS',
  'TOO_MANY_POINTS',
  'CROSSES_ANTIMERIDIAN',
  'TOO_WIDE',
  'TOO_LONG',
  'TOO_COMPLEX',
  'BAD_STATION',
  'TOO_MANY_STATIONS',
]);

function isInputCode(code: string): code is InputErrorCode {
  return INPUT_CODES.has(code);
}

/** How a message names station `index`: by its id, or by its place when it has none. */
export function stationName(id: RouteId | undefined, index: number): string {
  return id === undefined ? `Station at index ${index}` : `Station ${JSON.stringify(id)}`;
}

/**
 * The error a refusal gives the caller: an `InputError` that names the route (by `routeIds`) or
 * station (by `stationIds`) at fault, or an `InternalError` for a call the engine couldn't take.
 */
export function refusalError(
  refusal: Refusal,
  routeIds: readonly RouteId[],
  stationIds: readonly (RouteId | undefined)[],
): BraidError {
  const { code, reason, route, point, station } = refusal;
  if (!isInputCode(code)) {
    return internalError('INTERNAL', `the engine refused the call (${code}: ${reason}).`);
  }
  if (route !== undefined) {
    const routeId = routeIds[route];
    const who =
      routeId === undefined ? `Route at index ${route}` : `Route ${JSON.stringify(routeId)}`;
    const at = point === undefined ? '' : `, point ${point}`;
    return new InputError(code, `${who}${at}: ${reason}`, { routeId, index: route, point });
  }
  if (station !== undefined) {
    const stationId = stationIds[station];
    return new InputError(code, `${stationName(stationId, station)}: ${reason}`, {
      stationId,
      index: station,
    });
  }
  return new InputError(code, reason);
}

/**
 * Any failure of a call as the caller gets it: a refusal named by `refusalError`, a
 * transit-braid error as it is, anything else as an internal error.
 */
export function asBraidError(
  error: unknown,
  routeIds: readonly RouteId[],
  stationIds: readonly (RouteId | undefined)[],
): BraidError {
  if (error instanceof Refused) return refusalError(error.refusal, routeIds, stationIds);
  if (error instanceof BraidError) return error;
  return internalError('INTERNAL', error instanceof Error ? error.message : String(error));
}
