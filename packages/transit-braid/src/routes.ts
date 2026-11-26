// GeoJSON in and out of `sideBySide()`: checks the routes, flattens them for the worker, and
// builds the result. Pure functions, no worker, so they are tested in Node.

import { InputError, internalError } from './errors.ts';
import type { StationBars } from './stations.ts';

/** The most routes one `sideBySide()` call takes: a city's busiest routes. */
const MAX_ROUTES = 70;

/** The most points one call takes, all routes together, as in the core: checked here too, so
 * a huge input is refused before it is copied for the worker. */
const MAX_POINTS = 1_000_000;

/** A route's id: GeoJSON's own `id` field on the feature. */
export type RouteId = string | number;

/** A GeoJSON position: `[longitude, latitude]`, optionally followed by an altitude (ignored). */
export type Position = number[];

/** A GeoJSON LineString feature with an id. */
export interface LineFeature {
  type: 'Feature';
  id: RouteId;
  properties: Record<string, unknown> | null;
  geometry: { type: 'LineString'; coordinates: Position[] };
}

/** A GeoJSON FeatureCollection of LineStrings. */
export interface LineCollection {
  type: 'FeatureCollection';
  features: LineFeature[];
}

/**
 * What `sideBySide()` takes: a GeoJSON FeatureCollection of LineStrings. Shaped so a standard
 * GeoJSON type (e.g. `FeatureCollection<LineString>`) fits; `id` is optional here only for that
 * reason: a route without one is refused when called.
 */
export interface RoutesInput {
  type: 'FeatureCollection';
  features: readonly {
    type: 'Feature';
    id?: RouteId | undefined;
    properties: Record<string, unknown> | null;
    geometry: { type: 'LineString'; coordinates: readonly (readonly number[])[] };
  }[];
}

/** What `sideBySide()` returns. */
export interface SideBySideResult {
  /** One line per input route, in input order: same id, same direction, properties copied.
   * With `maxSideBySide`, the routes in `dropped` are left out. */
  lines: LineCollection;
  /** Station bars: one LineString across the bundle per station that has a route near,
   * with its id and properties; empty without the `stations` option. */
  stations: StationBars;
  /** The ids of the routes the lane cap (`maxSideBySide`) left out, in input order; empty
   * without a cap or when every route fits. */
  dropped: RouteId[];
}

/** Routes checked and flattened for the worker. */
export interface FlatRoutes {
  ids: RouteId[];
  properties: (Record<string, unknown> | null)[];
  /** Every point, flat: `[lng, lat, lng, lat, ...]`. */
  coords: Float64Array;
  /** Where each route starts in `coords`, counted in points. */
  starts: Uint32Array;
}

/**
 * Checks that `routes` is a FeatureCollection of at most 70 LineString features, each with a
 * unique `id` and numeric `[lng, lat]` positions, at most 1,000,000 positions together, and
 * flattens it. Value ranges (longitude, latitude, two distinct points, the date line, the spread
 * and length of the routes) are checked by the core, whose refusals name the route by its index.
 * Throws an `InputError` naming the route (by id, or by feature index when it has none).
 */
export function flattenRoutes(routes: unknown): FlatRoutes {
  if (!isObject(routes) || routes['type'] !== 'FeatureCollection') {
    throw new InputError('BAD_ROUTES', '`routes` must be a GeoJSON FeatureCollection.');
  }
  const features = routes['features'];
  if (!Array.isArray(features)) {
    throw new InputError('BAD_ROUTES', '`routes.features` must be an array.');
  }
  if (features.length > MAX_ROUTES) {
    throw new InputError(
      'TOO_MANY_ROUTES',
      `Too many routes: ${features.length} given, at most ${MAX_ROUTES} allowed.`,
    );
  }

  const flat: FlatRoutes = {
    ids: [],
    properties: [],
    coords: new Float64Array(),
    starts: new Uint32Array(),
  };
  const coords: number[] = [];
  const starts: number[] = [];
  const seen = new Map<RouteId, number>();
  let points = 0;
  for (const [index, feature] of (features as unknown[]).entries()) {
    const { id, properties, positions } = checkFeature(feature, index);
    const first = seen.get(id);
    if (first !== undefined) {
      throw new InputError(
        'DUPLICATE_ID',
        `Route ${JSON.stringify(id)} appears twice (features ${first} and ${index}).`,
        { routeId: id, index },
      );
    }
    seen.set(id, index);
    points += positions.length;
    if (points > MAX_POINTS) {
      throw new InputError(
        'TOO_MANY_POINTS',
        `Too many points: route ${JSON.stringify(id)} brings the total to ${points}, at most ${MAX_POINTS} allowed (all routes together).`,
        { routeId: id, index },
      );
    }
    flat.ids.push(id);
    flat.properties.push(properties);
    starts.push(coords.length / 2);
    for (const [point, position] of positions.entries()) {
      if (!isPosition(position)) {
        throw new InputError(
          'BAD_COORDINATE',
          `Route ${JSON.stringify(id)}, point ${point}: needs [longitude, latitude] numbers.`,
          { routeId: id, index, point },
        );
      }
      coords.push(position[0], position[1]);
    }
  }
  flat.coords = new Float64Array(coords);
  flat.starts = new Uint32Array(starts);
  return flat;
}

/** One feature's checked parts. */
interface CheckedFeature {
  id: RouteId;
  properties: Record<string, unknown> | null;
  positions: unknown[];
}

/** Checks one feature's shape; positions are checked by the caller. */
function checkFeature(feature: unknown, index: number): CheckedFeature {
  if (!isObject(feature) || feature['type'] !== 'Feature') {
    throw new InputError('BAD_ROUTES', `Feature ${index} is not a GeoJSON Feature.`, { index });
  }
  const id = feature['id'];
  if (!(typeof id === 'string' && id !== '') && !(typeof id === 'number' && Number.isFinite(id))) {
    throw new InputError(
      'MISSING_ID',
      `Feature ${index} needs an \`id\` (GeoJSON's own field): a non-empty string or a finite number.`,
      { index },
    );
  }
  const bad = (why: string): InputError =>
    new InputError('BAD_ROUTES', `Route ${JSON.stringify(id)}: ${why}`, { routeId: id, index });
  const props = feature['properties'];
  if (props !== null && props !== undefined && !isObject(props)) {
    throw bad('properties must be an object or null.');
  }
  const geometry = feature['geometry'];
  if (!isObject(geometry) || geometry['type'] !== 'LineString') {
    throw bad('geometry must be a LineString.');
  }
  const positions = geometry['coordinates'];
  if (!Array.isArray(positions)) throw bad('coordinates must be an array.');
  return { id, properties: isObject(props) ? props : null, positions };
}

/**
 * Builds `sideBySide()`'s result from the worker's flat answer: one LineString per route, with
 * the route's id and a shallow copy of its properties.
 */
export function buildResult(
  flat: Pick<FlatRoutes, 'ids' | 'properties'>,
  coords: Float64Array,
  starts: Uint32Array,
  dropped: Uint32Array = new Uint32Array(),
): SideBySideResult {
  const left = new Set(dropped);
  const kept = flat.ids.flatMap((id, i) => (left.has(i) ? [] : [{ id, i }]));
  const fits = left.size === dropped.length && kept.length + left.size === flat.ids.length;
  if (starts.length !== kept.length || !fits) {
    const cut = dropped.length === 0 ? '' : `, ${dropped.length} dropped`;
    throw internalError(
      'INTERNAL',
      `${starts.length} lines came back for ${flat.ids.length} routes${cut}.`,
    );
  }
  const features = kept.map(({ id, i }, k): LineFeature => {
    const start = starts[k] ?? 0;
    const end = starts[k + 1] ?? coords.length / 2;
    const positions: Position[] = [];
    for (let p = start; p < end; p++)
      positions.push([coords[2 * p] ?? NaN, coords[2 * p + 1] ?? NaN]);
    const properties = flat.properties[i] ?? null;
    return {
      type: 'Feature',
      id,
      properties: properties === null ? null : { ...properties },
      geometry: { type: 'LineString', coordinates: positions },
    };
  });
  return {
    lines: { type: 'FeatureCollection', features },
    stations: { type: 'FeatureCollection', features: [] },
    dropped: [...dropped].flatMap((i) => {
      const id = flat.ids[i];
      return id === undefined ? [] : [id];
    }),
  };
}

/** Past this many ids a numbering starts over: the next call is then matched by place once. */
const MAX_KEYS = 100_000;

/**
 * Numbers route ids for the engine: the same number for the same id on every call, so the
 * engine's cache can match an edit by id instead of by place in the list. Each session has its
 * own numbering (a plain `sideBySide()` call a fresh one), so no call depends on another's ids.
 * Ids compare strictly, as in `flattenRoutes` (`1` and `"1"` are two routes). The engine ignores
 * keys that repeat.
 */
export class RouteKeys {
  private readonly keyOf = new Map<string, number>();

  /** Each route's key, in order. */
  keysFor(ids: readonly RouteId[]): Uint32Array {
    if (this.keyOf.size + ids.length > MAX_KEYS) this.keyOf.clear();
    return Uint32Array.from(ids, (id) => {
      const name = `${typeof id}:${String(id)}`;
      let key = this.keyOf.get(name);
      if (key === undefined) {
        key = this.keyOf.size + 1;
        this.keyOf.set(name, key);
      }
      return key;
    });
  }
}

function isObject(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function isPosition(value: unknown): value is [number, number, ...number[]] {
  return (
    Array.isArray(value) &&
    value.length >= 2 &&
    value.every((n) => typeof n === 'number' && Number.isFinite(n))
  );
}
