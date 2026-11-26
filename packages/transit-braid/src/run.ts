// Runs a call on WASM that is already loaded, and keeps the caches: the sessions', and the plain
// call's one cache for a zoom change. Used inside the worker (`serve.ts`) and, on the caller's own
// thread, by the Node entry (`node.ts`).

import {
  __wbg_reset_state as resetWasm,
  EngineCache,
  Refusal as EngineRefusal,
} from '../wasm/transit_braid_wasm.js';
import { internalError, Refused, type Refusal } from './errors.ts';
import type { PipelineAnswer, SideBySideCall } from './protocol.ts';

/** True after a crash until a fresh WASM instance has replaced the crashed one. */
let crashed = false;

/** The open sessions' caches in this WASM instance, by session id; made on first use. */
const sessions = new Map<number, EngineCache>();

/**
 * Everything in a call that decides its line graph and its order: the routes (points, starts and
 * keys), the stations and the lane cap. The scale, line width and spacing rerun only the
 * geometry, so they are left out.
 */
interface GraphInput {
  coords: Float64Array;
  starts: Uint32Array;
  routeKeys: Uint32Array;
  stations: Float64Array;
  stationGroups: Uint32Array;
  maxSideBySide: number | undefined;
}

/**
 * The plain call's one cache, for a fast zoom: the last plain call's input and the engine's cache
 * built for it. A plain call with exactly that input (to the bit) reuses it at any scale, line
 * width or spacing, and the engine then redraws only the geometry, which gives the bits a fresh
 * cache gives. Any other input frees it first and draws from a new, empty cache, so a plain call
 * never starts from an earlier call's order. One per WASM instance (so one per worker, and one on
 * Node's own thread); a crash drops it with the instance, and a terminated worker frees it with
 * its thread.
 */
let plain: { input: GraphInput; cache: EngineCache } | undefined;

/** An empty `Uint32Array`, for keys and groups a call leaves out. */
const NONE = new Uint32Array();

/**
 * Runs one `sideBySide` call (the WASM must be loaded). A call that names no session uses the
 * plain call's cache when its input is the cache's (a zoom change), else a new, empty cache, which
 * it keeps for the next plain call: so the answer depends on nothing before it. A call that names
 * a session uses that session's cache, made on its first call.
 *
 * If the WASM crashes (a trap: a Rust panic, or no memory or stack left), its instance can't be
 * used again: its memory is never freed. So a fresh instance replaces it at once, and the call
 * throws an internal error. Every session's cache was in the crashed instance, so each session's
 * next call draws as a first call would: an edit is not warm-started from before.
 */
export function run(request: SideBySideCall): PipelineAnswer {
  if (request.type !== 'sideBySide') {
    // Only a caller bypassing the types gets here; answer with an error, never hang.
    throw internalError(
      'INTERNAL',
      `unknown request type ${String((request as { type: unknown }).type)}.`,
    );
  }
  try {
    // True only when the restart right after a crash failed (no memory left even for a fresh
    // instance).
    if (crashed) restart();
    return call(request);
  } catch (error) {
    if (error instanceof EngineRefusal) throw new Refused(read(error));
    crashed = true;
    try {
      restart();
    } catch {
      // No memory left even for a fresh instance: `crashed` stays true, the next call tries again.
    }
    throw internalError(
      'ENGINE_CRASHED',
      `the engine crashed (${String(error)}). It restarted, so the next call starts fresh.`,
    );
  }
}

/** Frees session `id`'s cache; nothing for a session this instance doesn't have. */
export function closeSession(id: number): void {
  const session = sessions.get(id);
  sessions.delete(id);
  session?.free();
}

/**
 * Swaps the WASM instance for a fresh one: new memory, no caches. The sessions' objects and the
 * plain call's cache pointed into the old memory, so they are dropped, never freed (freeing one
 * would write into the new instance); each gets a new cache on its next call.
 */
function restart(): void {
  sessions.clear();
  plain = undefined;
  resetWasm();
  crashed = false;
}

/** Frees the plain call's cache, if any. */
function forgetPlain(): void {
  plain?.cache.free();
  plain = undefined;
}

/** The part of `request` that decides the graph and the order, copied, so nothing can change it. */
function graphInput(request: SideBySideCall): GraphInput {
  return {
    coords: request.coords.slice(),
    starts: request.starts.slice(),
    routeKeys: (request.routeKeys ?? NONE).slice(),
    stations: request.stations.slice(),
    stationGroups: (request.stationGroups ?? NONE).slice(),
    maxSideBySide: request.maxSideBySide,
  };
}

/** True when `request` has exactly `input`: every number the same to the bit. */
function sameGraphInput(input: GraphInput, request: SideBySideCall): boolean {
  return (
    input.maxSideBySide === request.maxSideBySide &&
    sameBits(input.starts, request.starts) &&
    sameBits(input.routeKeys, request.routeKeys ?? NONE) &&
    sameBits(input.stationGroups, request.stationGroups ?? NONE) &&
    sameBits(input.coords, request.coords) &&
    sameBits(input.stations, request.stations)
  );
}

/**
 * True when `a` and `b` hold the same bytes. Compared as 32-bit words, so two numbers are the same
 * only with the same bits: 0 and -0 differ, a NaN equals only the same NaN.
 */
function sameBits(a: Float64Array | Uint32Array, b: Float64Array | Uint32Array): boolean {
  if (a.byteLength !== b.byteLength) return false;
  const x = new Uint32Array(a.buffer, a.byteOffset, a.byteLength / 4);
  const y = new Uint32Array(b.buffer, b.byteOffset, b.byteLength / 4);
  for (let i = 0; i < x.length; i++) {
    if (x[i] !== y[i]) return false;
  }
  return true;
}

/**
 * The cache for `request`: its session's; or, for a plain call, the plain call's cache when the
 * input is the same, else a new one (`fresh`), after freeing the old one so the two never take
 * memory at once.
 */
function cacheFor(request: SideBySideCall): { cache: EngineCache; fresh: boolean } {
  if (request.session !== undefined) return { cache: sessionCache(request.session), fresh: false };
  if (plain !== undefined && sameGraphInput(plain.input, request)) {
    return { cache: plain.cache, fresh: false };
  }
  forgetPlain();
  return { cache: new EngineCache(), fresh: true };
}

/**
 * The engine's refusal as plain data (it may cross to the main thread), freeing its Rust side.
 * A refusal leaves the WASM working; anything else the engine throws is a crash:
 * `WebAssembly.RuntimeError` for a trap, `RangeError` for no stack left.
 */
function read(refusal: EngineRefusal): Refusal {
  try {
    return {
      code: refusal.code,
      reason: refusal.reason,
      route: refusal.route,
      point: refusal.point,
      station: refusal.station,
    };
  } finally {
    refusal.free();
  }
}

/** The session `id`'s cache, made on its first call. */
function sessionCache(id: number): EngineCache {
  let cache = sessions.get(id);
  if (cache === undefined) {
    cache = new EngineCache();
    sessions.set(id, cache);
  }
  return cache;
}

function call(request: SideBySideCall): PipelineAnswer {
  const { cache, fresh } = cacheFor(request);
  let out;
  try {
    out = cache.sideBySide(
      request.coords,
      request.starts,
      request.routeKeys ?? NONE,
      request.stations,
      request.stationGroups ?? NONE,
      request.zoom,
      request.metersPerPixel,
      request.lineWidth,
      request.spacing,
      request.maxSideBySide,
    );
  } catch (error) {
    // A refusal leaves the engine working, and a cache that was used before as it was: free
    // only a new plain cache, which nothing keeps. After a crash nothing is freed: the restart
    // drops the whole instance.
    if (fresh && error instanceof EngineRefusal) cache.free();
    throw error;
  }
  if (fresh) plain = { input: graphInput(request), cache };
  try {
    // Each array moves out of the result (Rust keeps no copy), then the result itself is freed.
    return {
      coords: out.takeCoords(),
      starts: out.takeStarts(),
      bars: out.takeBars(),
      dropped: out.takeDropped(),
    };
  } finally {
    out.free();
  }
}
