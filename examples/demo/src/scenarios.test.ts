import { describe, expect, it } from 'vitest';

import { parseScenario, SCENARIOS, toLngLat } from './scenarios.ts';

const fixture = (braid: Record<string, unknown>, features: unknown[]) =>
  JSON.stringify({ type: 'FeatureCollection', braid, features });

const line = (name: string, coordinates: unknown) => ({
  type: 'Feature',
  properties: { line: name, color: '#e4002b' },
  geometry: { type: 'LineString', coordinates },
});

describe('SCENARIOS', () => {
  it('reads all 12 fixtures, each with routes and the notes the Examples page shows', () => {
    expect(SCENARIOS.map((s) => s.id)).toEqual([
      'y-split',
      'parallel-corridor',
      'x-crossing',
      'dog-bone',
      'hairpin',
      'opposite-direction',
      'loop-line',
      'loop-twice',
      'one-way-pair',
      'out-and-back',
      'stops',
      'stress-grid',
    ]);
    for (const s of SCENARIOS) {
      expect(s.routes.length).toBeGreaterThanOrEqual(2);
      expect(new Set(s.routes.map((r) => r.id)).size).toBe(s.routes.length);
      expect(s.title).not.toBe('');
      expect(s.description).not.toBe('');
      expect(s.look).not.toBe('');
    }
  });

  it('keeps the stress grid at 70 routes, the most one call takes', () => {
    expect(SCENARIOS.find((s) => s.id === 'stress-grid')?.routes).toHaveLength(70);
  });

  it('turns the stops fixture into stations, with names where they have one', () => {
    const stops = SCENARIOS.find((s) => s.id === 'stops')?.stations?.features ?? [];
    expect(stops).toHaveLength(8);
    expect(stops.filter((f) => f.properties?.['name'] === 'Main St')).toHaveLength(3);
    expect(stops.filter((f) => f.properties?.['name'] === undefined)).toHaveLength(2);
  });

  it('places every point inside its bounds', () => {
    for (const s of SCENARIOS) {
      const [[w, south], [e, north]] = s.bounds;
      for (const [lng, lat] of s.routes.flatMap((r) => r.points)) {
        expect(lng).toBeGreaterThanOrEqual(w);
        expect(lng).toBeLessThanOrEqual(e);
        expect(lat).toBeGreaterThanOrEqual(south);
        expect(lat).toBeLessThanOrEqual(north);
      }
    }
  });
});

describe('toLngLat', () => {
  it('maps meters from 0°, 0° to degrees: 1 km east is about 0.009°', () => {
    expect(toLngLat(0, 0)).toEqual([0, 0]);
    expect(toLngLat(1000, 0)[0]).toBeCloseTo(0.008983, 6);
    expect(toLngLat(0, -1000)[1]).toBeCloseTo(-0.009044, 6);
  });
});

describe('parseScenario', () => {
  it('names routes by their line, colors them, and skips what is not a route', () => {
    const s = parseScenario(
      fixture({ name: 'test', units: 'meters', description: 'd', expect: 'e' }, [
        line('A', [
          [0, 0],
          [100, 0],
        ]),
        line('B', [[0, 0]]),
        { type: 'Feature', properties: {}, geometry: { type: 'Point', coordinates: [0, 0] } },
        null,
        {
          type: 'Feature',
          properties: {},
          geometry: {
            type: 'LineString',
            coordinates: [
              [0, 0],
              [5, 5],
            ],
          },
        },
      ]),
    );
    expect(s.routes.map((r) => r.id)).toEqual(['A', 'route-5']);
    expect(s.routes[0]?.color).toEqual([228, 0, 43]);
    expect(s.stations).toBeNull();
    expect(s.title).toBe('test');
  });

  it('refuses text that is not a fixture in meters, naming why', () => {
    const ok = [
      line('A', [
        [0, 0],
        [1, 1],
      ]),
    ];
    expect(() => parseScenario('not json')).toThrow(SyntaxError);
    expect(() => parseScenario('null')).toThrow('Not a fixture');
    expect(() => parseScenario(JSON.stringify({ features: ok }))).toThrow('Not a fixture');
    expect(() => parseScenario(fixture({ units: 'meters' }, ok))).toThrow('braid.name');
    expect(() => parseScenario(fixture({ name: 'x', units: 'degrees' }, ok))).toThrow(
      'only fixtures in meters',
    );
    expect(() => parseScenario(fixture({ name: 'x', units: 'meters' }, []))).toThrow(
      'no route with two or more points',
    );
  });
});
