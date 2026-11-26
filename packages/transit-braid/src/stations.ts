// Stations in and station bars out of `sideBySide()`: checks the stations, flattens them
// for the worker, and builds the bars. Pure functions, tested in Node.

import { InputError, internalError, stationName } from './errors.ts';
import type { Position, RouteId } from './routes.ts';

/** The most stations one call takes (as the core: a mistaken input can't make a call slow). */
const MAX_STATIONS = 5_000;

/**
 * `sideBySide()`'s `stations` option: a GeoJSON FeatureCollection of Points, each with an
 * optional `id` and properties (for example `name`). Shaped so a standard GeoJSON type
 * (`FeatureCollection<Point>`) fits.
 */
export interface StationsInput {
  type: 'FeatureCollection';
  features: readonly {
    type: 'Feature';
    id?: RouteId | undefined;
    properties: Record<string, unknown> | null;
    geometry: { type: 'Point'; coordinates: readonly number[] };
  }[];
}

/** A station's bar: a LineString across the whole bundle where the station is, with the
 * station's id (when it has one) and a copy of its properties. */
export interface StationBar {
  type: 'Feature';
  id?: RouteId;
  properties: Record<string, unknown> | null;
  geometry: { type: 'LineString'; coordinates: Position[] };
}

/** The bars `sideBySide()` returns in `stations`. */
export interface StationBars {
  type: 'FeatureCollection';
  features: StationBar[];
}

/** Stations checked and flattened for the worker. */
export interface FlatStations {
  /** Each station's id, or undefined when it has none. */
  ids: (RouteId | undefined)[];
  properties: (Record<string, unknown> | null)[];
  /** One point per station, flat: `[lng, lat, lng, lat, ...]`. */
  coords: Float64Array;
  /**
   * Each station's name as a number: stations whose `properties.name` is the same non-empty
   * string share a number (from 1, in order of first appearance); 0 for a station without one.
   * Same-name stops close together share a bar.
   */
  groups: Uint32Array;
}

/**
 * Checks the `stations` option and flattens it. No stations (undefined or null) gives none.
 * Throws an `InputError` naming the station (by id, or by index) for anything that isn't a
 * FeatureCollection of at most 5,000 Point features with numeric `[lng, lat]`. Ranges
 * (longitude, latitude) are checked by the core, whose refusals name the station by its index.
 */
export function flattenStations(stations: unknown): FlatStations {
  const flat: FlatStations = {
    ids: [],
    properties: [],
    coords: new Float64Array(),
    groups: new Uint32Array(),
  };
  if (stations === undefined || stations === null) return flat;
  if (!isObject(stations) || stations['type'] !== 'FeatureCollection') {
    throw new InputError(
      'BAD_STATION',
      '`stations` must be a GeoJSON FeatureCollection of Points.',
    );
  }
  const features = stations['features'];
  if (!Array.isArray(features)) {
    throw new InputError('BAD_STATION', '`stations.features` must be an array.');
  }
  if (features.length > MAX_STATIONS) {
    throw new InputError(
      'TOO_MANY_STATIONS',
      `Too many stations: ${features.length} given, at most ${MAX_STATIONS} allowed.`,
    );
  }
  const coords: number[] = [];
  const groups: number[] = [];
  const groupOf = new Map<string, number>();
  for (const [index, feature] of (features as unknown[]).entries()) {
    if (!isObject(feature) || feature['type'] !== 'Feature') {
      throw new InputError('BAD_STATION', `Station at index ${index} is not a GeoJSON Feature.`, {
        index,
      });
    }
    const rawId = feature['id'];
    let id: RouteId | undefined;
    if (rawId !== undefined && rawId !== null) {
      const ok =
        (typeof rawId === 'string' && rawId !== '') ||
        (typeof rawId === 'number' && Number.isFinite(rawId));
      if (!ok) {
        throw new InputError(
          'BAD_STATION',
          `Station at index ${index}: \`id\` must be a non-empty string or a finite number.`,
          { index },
        );
      }
      id = rawId;
    }
    const bad = (why: string): InputError =>
      new InputError('BAD_STATION', `${stationName(id, index)}: ${why}`, {
        stationId: id,
        index,
      });
    const props = feature['properties'];
    if (props !== null && props !== undefined && !isObject(props)) {
      throw bad('properties must be an object or null.');
    }
    const geometry = feature['geometry'];
    if (!isObject(geometry) || geometry['type'] !== 'Point') {
      throw bad('geometry must be a Point.');
    }
    const position = geometry['coordinates'];
    if (!isPosition(position)) throw bad('needs [longitude, latitude] numbers.');
    flat.ids.push(id);
    flat.properties.push(isObject(props) ? props : null);
    coords.push(position[0], position[1]);
    const stopName = isObject(props) ? props['name'] : undefined;
    if (typeof stopName === 'string' && stopName !== '') {
      if (!groupOf.has(stopName)) groupOf.set(stopName, groupOf.size + 1);
      groups.push(groupOf.get(stopName) ?? 0);
    } else {
      groups.push(0);
    }
  }
  flat.coords = new Float64Array(coords);
  flat.groups = new Uint32Array(groups);
  return flat;
}

/**
 * The bars from the worker's flat answer (four numbers per station, NaN for a station with no
 * route near): one LineString per station that has a bar, in station order.
 */
export function buildBars(
  flat: Pick<FlatStations, 'ids' | 'properties'>,
  bars: Float64Array,
): StationBars {
  if (bars.length !== 4 * flat.ids.length) {
    throw internalError(
      'INTERNAL',
      `${bars.length / 4} bars came back for ${flat.ids.length} stations.`,
    );
  }
  const features: StationBar[] = [];
  for (const [i, id] of flat.ids.entries()) {
    const [x1 = NaN, y1 = NaN, x2 = NaN, y2 = NaN] = bars.subarray(4 * i, 4 * i + 4);
    if (![x1, y1, x2, y2].every(Number.isFinite)) continue;
    const properties = flat.properties[i] ?? null;
    const bar: StationBar = {
      type: 'Feature',
      properties: properties === null ? null : { ...properties },
      geometry: {
        type: 'LineString',
        coordinates: [
          [x1, y1],
          [x2, y2],
        ],
      },
    };
    if (id !== undefined) bar.id = id;
    features.push(bar);
  }
  return { type: 'FeatureCollection', features };
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
