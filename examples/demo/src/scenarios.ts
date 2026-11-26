// The repo's own test scenarios (`fixtures/`, MIT): made-up routes in meters, each showing one
// thing transit-braid has to get right. The demo draws them around 0°, 0° on a plain background,
// and the Examples page lists them. Pure, so it is tested in Node.
import dogBone from '../../../fixtures/synthetic/dog-bone.geojson?raw';
import hairpin from '../../../fixtures/synthetic/hairpin.geojson?raw';
import oppositeDirection from '../../../fixtures/synthetic/opposite-direction.geojson?raw';
import parallelCorridor from '../../../fixtures/synthetic/parallel-corridor.geojson?raw';
import xCrossing from '../../../fixtures/synthetic/x-crossing.geojson?raw';
import ySplit from '../../../fixtures/synthetic/y-split.geojson?raw';
import loopLine from '../../../fixtures/bus/loop-line.geojson?raw';
import loopTwice from '../../../fixtures/bus/loop-twice.geojson?raw';
import oneWayPair from '../../../fixtures/bus/one-way-pair.geojson?raw';
import outAndBack from '../../../fixtures/bus/out-and-back.geojson?raw';
import stops from '../../../fixtures/bus/stops.geojson?raw';
import stressGrid from '../../../fixtures/stress/stress-grid.geojson?raw';
import type { StationsInput } from 'transit-braid';

import { type DemoRoute, hexToRgb, isRecord, PALETTE } from './edit.ts';
import type { LngLat } from './project.ts';

export interface Scenario {
  id: string;
  title: string;
  /** What the routes do, in one or two sentences. */
  description: string;
  /** What a good drawing shows. */
  look: string;
  routes: DemoRoute[];
  stations: StationsInput | null;
  /** The south-west and north-east corners of the routes and stops. */
  bounds: [LngLat, LngLat];
}

// A flat projection is plenty: the scenarios are at most a few kilometers wide.
const M_PER_DEG_LNG = 111_320;
const M_PER_DEG_LAT = 110_574;

/** A point in meters from 0°, 0° as `[longitude, latitude]`. */
export const toLngLat = (x: number, y: number): LngLat => [x / M_PER_DEG_LNG, y / M_PER_DEG_LAT];

/** Short titles for the Examples page and the demo's picker; any other fixture keeps its name. */
const TITLES: Readonly<Record<string, string>> = {
  'y-split': 'Y-split',
  'parallel-corridor': 'Three lanes through a bend',
  'x-crossing': 'Two corridors crossing',
  'dog-bone': 'Meet, share, part',
  hairpin: 'A hairpin bend',
  'opposite-direction': 'Opposite directions',
  'loop-line': 'A loop line',
  'loop-twice': 'Around a block, then again',
  'one-way-pair': 'A one-way street pair',
  'out-and-back': 'Out and back',
  stops: 'Stops on both sides of a street',
  'stress-grid': 'Stress grid: 70 routes',
};

const isXY = (p: unknown): p is [number, number] =>
  Array.isArray(p) && typeof p[0] === 'number' && typeof p[1] === 'number';

/**
 * One fixture file (a GeoJSON FeatureCollection in meters, with its `braid` notes) as a scenario.
 * Each LineString becomes a route named by its `line` property; `braid.stations` become stops.
 * Throws, naming the problem, when the text isn't such a fixture.
 */
export function parseScenario(text: string): Scenario {
  const data: unknown = JSON.parse(text);
  if (!isRecord(data) || !isRecord(data['braid']) || !Array.isArray(data['features'])) {
    throw new Error('Not a fixture: a FeatureCollection with "braid" notes is expected');
  }
  const braid = data['braid'];
  const id = braid['name'];
  if (typeof id !== 'string' || id === '') throw new Error('A fixture needs a "braid.name"');
  if (braid['units'] !== 'meters') throw new Error(`${id}: only fixtures in meters are drawn`);
  const features: readonly unknown[] = data['features'];
  const routes: DemoRoute[] = [];
  for (const [i, f] of features.entries()) {
    if (!isRecord(f) || !isRecord(f['geometry']) || f['geometry']['type'] !== 'LineString')
      continue;
    const coordinates = f['geometry']['coordinates'];
    if (!Array.isArray(coordinates)) continue;
    const given: readonly unknown[] = coordinates;
    const points = given.filter(isXY).map(([x, y]) => toLngLat(x, y));
    if (points.length < 2) continue;
    const props = isRecord(f['properties']) ? f['properties'] : {};
    const line = props['line'];
    routes.push({
      id: typeof line === 'string' && line !== '' ? line : `route-${i + 1}`,
      color: hexToRgb(props['color']) ?? PALETTE[routes.length % PALETTE.length] ?? [0, 0, 0],
      points,
    });
  }
  if (routes.length === 0) throw new Error(`${id}: no route with two or more points`);

  const listed: readonly unknown[] = Array.isArray(braid['stations']) ? braid['stations'] : [];
  const stopFeatures: StationsInput['features'][number][] = [];
  for (const s of listed) {
    if (!isRecord(s) || !isXY(s['at'])) continue;
    const name = s['name'];
    stopFeatures.push({
      type: 'Feature',
      properties: typeof name === 'string' ? { name } : {},
      geometry: { type: 'Point', coordinates: toLngLat(s['at'][0], s['at'][1]) },
    });
  }

  const all: LngLat[] = [
    ...routes.flatMap((r) => r.points),
    ...stopFeatures.map((s): LngLat => [
      s.geometry.coordinates[0] ?? 0,
      s.geometry.coordinates[1] ?? 0,
    ]),
  ];
  const lngs = all.map((p) => p[0]);
  const lats = all.map((p) => p[1]);
  const description = braid['description'];
  const look = braid['expect'];
  return {
    id,
    title: TITLES[id] ?? id,
    description: typeof description === 'string' ? description : '',
    look: typeof look === 'string' ? look : '',
    routes,
    stations:
      stopFeatures.length === 0 ? null : { type: 'FeatureCollection', features: stopFeatures },
    bounds: [
      [Math.min(...lngs), Math.min(...lats)],
      [Math.max(...lngs), Math.max(...lats)],
    ],
  };
}

/** The scenarios, simplest first, in the order the Examples page lists them. */
export const SCENARIOS: readonly Scenario[] = [
  ySplit,
  parallelCorridor,
  xCrossing,
  dogBone,
  hairpin,
  oppositeDirection,
  loopLine,
  loopTwice,
  oneWayPair,
  outAndBack,
  stops,
  stressGrid,
].map(parseScenario);
