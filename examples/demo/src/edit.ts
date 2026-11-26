// The demo's routes and the edits a user can make: add, remove, and move a route's points.
// Pure functions, so they are tested in Node; the map wiring is in main.ts.
import type { RoutesInput } from 'transit-braid';

import type { LngLat } from './project.ts';

export type Rgb = [number, number, number];

export interface DemoRoute {
  id: string;
  /** What the route list shows; the id when absent. */
  name?: string;
  color: Rgb;
  points: LngLat[];
}

/** Colors for routes, in the order they are given out. */
export const PALETTE: readonly Rgb[] = [
  [228, 0, 43],
  [0, 114, 206],
  [242, 169, 0],
  [0, 151, 57],
  [117, 59, 189],
  [0, 163, 173],
  [229, 114, 0],
  [101, 101, 101],
];

/** Three made-up routes in central Tel Aviv: they share a trunk, then split three ways. */
export const SAMPLE_ROUTES: readonly DemoRoute[] = [
  {
    id: 'red',
    color: [228, 0, 43],
    points: [
      [34.7705, 32.0663],
      [34.7745, 32.0712],
      [34.7768, 32.0775],
      [34.7812, 32.0809],
      [34.7826, 32.0868],
    ],
  },
  {
    id: 'blue',
    color: [0, 114, 206],
    points: [
      [34.7705, 32.0663],
      [34.7745, 32.0712],
      [34.7768, 32.0775],
      [34.7885, 32.0752],
    ],
  },
  {
    id: 'yellow',
    color: [242, 169, 0],
    points: [
      [34.7705, 32.0663],
      [34.7745, 32.0712],
      [34.7768, 32.0775],
      [34.7731, 32.0851],
    ],
  },
];

/** Adds a route through `points` with the next free id (`route-1`, `route-2`, …) and the next
 * palette color. Returns the reason instead when it has fewer than two distinct points. */
export function addRoute(
  routes: readonly DemoRoute[],
  points: readonly LngLat[],
): DemoRoute[] | string {
  const first = points[0];
  if (first === undefined || points.every((p) => p[0] === first[0] && p[1] === first[1])) {
    return 'A route needs at least two different points.';
  }
  const used = new Set(routes.map((r) => r.id));
  let n = 1;
  while (used.has(`route-${n}`)) n++;
  const color = PALETTE[routes.length % PALETTE.length] ?? [0, 0, 0];
  return [...routes, { id: `route-${n}`, color, points: points.map(([x, y]): LngLat => [x, y]) }];
}

/** Every route but `id` (unchanged when there is none). */
export function removeRoute(routes: readonly DemoRoute[], id: string): DemoRoute[] {
  return routes.filter((r) => r.id !== id);
}

/** Route `id` with its point `index` moved to `to` (unchanged when either doesn't exist). */
export function movePoint(
  routes: readonly DemoRoute[],
  id: string,
  index: number,
  to: LngLat,
): DemoRoute[] {
  return routes.map((r) =>
    r.id !== id || index < 0 || index >= r.points.length
      ? r
      : { ...r, points: r.points.map((p, i): LngLat => (i === index ? [to[0], to[1]] : p)) },
  );
}

/** `#rrggbb` as `[r, g, b]`; null for anything else. */
export function hexToRgb(hex: unknown): Rgb | null {
  if (typeof hex !== 'string' || !/^#[0-9a-f]{6}$/i.test(hex)) return null;
  const n = Number.parseInt(hex.slice(1), 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
}

/** A plain object (not null, not an array). */
export const isRecord = (v: unknown): v is Record<string, unknown> =>
  typeof v === 'object' && v !== null && !Array.isArray(v);

const isLngLat = (p: unknown): p is LngLat =>
  Array.isArray(p) && typeof p[0] === 'number' && typeof p[1] === 'number';

/**
 * The routes of a GeoJSON FeatureCollection: each LineString feature with an id becomes a route,
 * named by its `name` property and colored by its `color` (`#rrggbb`), or the palette's next
 * color. Anything else (another geometry, no id, fewer than two points) is skipped.
 */
export function routesFromGeoJson(data: unknown): DemoRoute[] {
  if (!isRecord(data)) return [];
  const features = data['features'];
  if (!Array.isArray(features)) return [];
  const list: readonly unknown[] = features;
  const routes: DemoRoute[] = [];
  for (const f of list) {
    if (!isRecord(f)) continue;
    const id = f['id'];
    const geometry = f['geometry'];
    if (typeof id !== 'string' && typeof id !== 'number') continue;
    if (!isRecord(geometry) || geometry['type'] !== 'LineString') continue;
    const coordinates = geometry['coordinates'];
    if (!Array.isArray(coordinates)) continue;
    const given: readonly unknown[] = coordinates;
    const points = given.filter(isLngLat);
    if (points.length < 2) continue;
    const props = isRecord(f['properties']) ? f['properties'] : {};
    const name = props['name'];
    routes.push({
      id: String(id),
      ...(typeof name === 'string' ? { name } : {}),
      color: hexToRgb(props['color']) ?? PALETTE[routes.length % PALETTE.length] ?? [0, 0, 0],
      points: points.map(([x, y]): LngLat => [x, y]),
    });
  }
  return routes;
}

/** The routes as `sideBySide()`'s input: one LineString each, with its id and color. */
export function toRoutesInput(routes: readonly DemoRoute[]): RoutesInput {
  return {
    type: 'FeatureCollection',
    features: routes.map((r) => ({
      type: 'Feature',
      id: r.id,
      properties: { color: r.color },
      geometry: { type: 'LineString', coordinates: r.points },
    })),
  };
}

/** A point on a drawn line, for tests to ask the map what is drawn there: the middle of its first
 * segment. Null when the line has no segment. */
export function probeOf(path: readonly LngLat[]): LngLat | null {
  const [a, b] = path;
  return a && b ? [(a[0] + b[0]) / 2, (a[1] + b[1]) / 2] : null;
}
