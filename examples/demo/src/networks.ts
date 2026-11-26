// The networks the demo shows: three made-up routes to edit, real bus routes in Tel Aviv with
// their stops, and the repo's test scenarios. Real data loads only when picked, from the site's
// own folder.
import type { StationsInput } from 'transit-braid';

import { type DemoRoute, isRecord, routesFromGeoJson, SAMPLE_ROUTES } from './edit.ts';
import type { LngLat } from './project.ts';
import { SCENARIOS } from './scenarios.ts';

export interface Network {
  id: string;
  name: string;
  /** One or two plain sentences under the network picker. */
  about: string;
  /** Where the data comes from, shown with it (real data only). */
  credit?: string;
  center: LngLat;
  zoom: number;
  /** The map under the routes: real streets (the default), or a plain background. */
  basemap?: 'streets' | 'plain';
  /** What a good drawing shows (the scenarios). */
  look?: string;
  /** Fit the view to these corners (south-west, north-east) instead of `center` and `zoom`. */
  bounds?: [LngLat, LngLat];
  load: () => Promise<{ routes: DemoRoute[]; stations: StationsInput | null }>;
}

/** The repo's test scenarios as networks: made-up routes on a plain background. */
const SCENARIO_NETWORKS: readonly Network[] = SCENARIOS.map((s) => ({
  id: s.id,
  name: s.title,
  about: s.description,
  look: s.look,
  basemap: 'plain',
  bounds: s.bounds,
  center: [(s.bounds[0][0] + s.bounds[1][0]) / 2, (s.bounds[0][1] + s.bounds[1][1]) / 2],
  zoom: 16,
  load: async () => ({ routes: structuredClone(s.routes), stations: s.stations }),
}));

/** A GeoJSON file next to the page (relative, so it works under any path). */
async function fetchJson(path: string): Promise<unknown> {
  const response = await fetch(path);
  if (!response.ok) throw new Error(`${path}: ${response.status} ${response.statusText}`);
  return response.json();
}

/** The Points of a FeatureCollection as `sideBySide()`'s `stations` (null when there are none). */
export function stationsFromGeoJson(data: unknown): StationsInput | null {
  if (!isRecord(data)) return null;
  const features = data['features'];
  if (!Array.isArray(features)) return null;
  const list: readonly unknown[] = features;
  const points: StationsInput['features'][number][] = [];
  for (const f of list) {
    if (!isRecord(f) || f['type'] !== 'Feature') continue;
    const geometry = f['geometry'];
    if (!isRecord(geometry) || geometry['type'] !== 'Point') continue;
    const coordinates = geometry['coordinates'];
    if (!Array.isArray(coordinates)) continue;
    const given: readonly unknown[] = coordinates;
    if (!given.every((c): c is number => typeof c === 'number')) continue;
    const id = f['id'];
    const properties = f['properties'];
    points.push({
      type: 'Feature',
      ...(typeof id === 'string' || typeof id === 'number' ? { id } : {}),
      properties: isRecord(properties) ? properties : null,
      geometry: { type: 'Point', coordinates: given },
    });
  }
  return points.length === 0 ? null : { type: 'FeatureCollection', features: points };
}

export const NETWORKS: readonly Network[] = [
  {
    id: 'sample',
    name: 'Three routes to edit',
    about:
      'Three made-up routes in central Tel Aviv: they share a street, then split three ways. ' +
      'Move, add or remove them.',
    center: [34.7775, 32.0765],
    zoom: 14.5,
    load: async () => ({ routes: structuredClone([...SAMPLE_ROUTES]), stations: null }),
  },
  {
    id: 'tel-aviv',
    name: 'Tel Aviv: 25 bus routes',
    about:
      '25 of the busiest weekday bus routes that run mostly inside Tel Aviv-Yafo, one direction ' +
      'each, and their 711 stops.',
    credit:
      'Data: Israel public transport GTFS, Ministry of Transport and Road Safety (weekday ' +
      'schedule of 24 Nov 2026). It may contain errors or be out of date. Not affiliated with or ' +
      'endorsed by the Ministry of Transport.',
    center: [34.7818, 32.0795],
    zoom: 14,
    load: async () => {
      const [routes, stops] = await Promise.all([
        fetchJson('data/tel-aviv-routes.geojson'),
        fetchJson('data/tel-aviv-stops.geojson'),
      ]);
      return { routes: routesFromGeoJson(routes), stations: stationsFromGeoJson(stops) };
    },
  },
  ...SCENARIO_NETWORKS,
];
