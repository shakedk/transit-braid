import { readFileSync } from 'node:fs';
import { join } from 'node:path';

import { describe, expect, it } from 'vitest';

import { NETWORKS, stationsFromGeoJson } from './networks.ts';

const point = (coordinates: unknown, extra: Record<string, unknown> = {}) => ({
  type: 'Feature',
  properties: { name: 'Main St' },
  geometry: { type: 'Point', coordinates },
  ...extra,
});

describe('stationsFromGeoJson', () => {
  it('keeps the Points, with their id and properties', () => {
    const out = stationsFromGeoJson({
      type: 'FeatureCollection',
      features: [point([34.8, 32.0], { id: 's1' }), point([34.81, 32.01])],
    });
    expect(out).toEqual({
      type: 'FeatureCollection',
      features: [
        {
          type: 'Feature',
          id: 's1',
          properties: { name: 'Main St' },
          geometry: { type: 'Point', coordinates: [34.8, 32.0] },
        },
        {
          type: 'Feature',
          properties: { name: 'Main St' },
          geometry: { type: 'Point', coordinates: [34.81, 32.01] },
        },
      ],
    });
  });

  it('skips other geometries, bad coordinates and non-features', () => {
    const out = stationsFromGeoJson({
      type: 'FeatureCollection',
      features: [
        point(['34.8', 32.0]),
        point('nowhere'),
        { type: 'Feature', properties: {}, geometry: { type: 'LineString', coordinates: [] } },
        { type: 'NotAFeature', geometry: { type: 'Point', coordinates: [1, 2] } },
        null,
        point([1, 2], { properties: 'not an object' }),
      ],
    });
    expect(out?.features).toEqual([
      { type: 'Feature', properties: null, geometry: { type: 'Point', coordinates: [1, 2] } },
    ]);
  });

  it('gives null when there are no stations', () => {
    for (const bad of [null, undefined, 1, 'x', [], {}, { features: [] }, { features: 'no' }]) {
      expect(stationsFromGeoJson(bad)).toBeNull();
    }
  });

  it('reads all 711 Tel Aviv stops', () => {
    const text = readFileSync(
      join(import.meta.dirname, '..', 'public', 'data', 'tel-aviv-stops.geojson'),
      'utf8',
    );
    expect(stationsFromGeoJson(JSON.parse(text))?.features).toHaveLength(711);
  });
});

describe('NETWORKS', () => {
  it('starts with the routes to edit, and credits the real data', () => {
    expect(NETWORKS.map((n) => n.id).slice(0, 2)).toEqual(['sample', 'tel-aviv']);
    expect(NETWORKS[0]?.credit).toBeUndefined();
    expect(NETWORKS[1]?.credit).toContain('Ministry of Transport');
    expect(NETWORKS[1]?.credit).toContain('Not affiliated with or endorsed');
  });

  it('loads the sample routes as a copy, so edits never change the originals', async () => {
    const sample = NETWORKS[0];
    if (!sample) throw new Error('no sample network');
    const a = await sample.load();
    const b = await sample.load();
    expect(a.routes.map((r) => r.id)).toEqual(['red', 'blue', 'yellow']);
    expect(a.stations).toBeNull();
    a.routes[0]?.points.splice(0);
    expect(b.routes[0]?.points.length).toBeGreaterThan(0);
  });

  it('ends with the 12 scenarios, each on a plain background and framed by its bounds', () => {
    const scenarios = NETWORKS.slice(2);
    expect(scenarios).toHaveLength(12);
    for (const n of scenarios) {
      expect(n.basemap).toBe('plain');
      expect(n.bounds).toBeDefined();
      expect(n.look).not.toBe('');
    }
    expect(NETWORKS.slice(0, 2).every((n) => n.basemap !== 'plain')).toBe(true);
  });
});
