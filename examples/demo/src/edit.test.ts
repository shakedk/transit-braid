import { readFileSync } from 'node:fs';
import { join } from 'node:path';

import { describe, expect, it } from 'vitest';

import {
  addRoute,
  type DemoRoute,
  hexToRgb,
  movePoint,
  PALETTE,
  probeOf,
  removeRoute,
  routesFromGeoJson,
  SAMPLE_ROUTES,
  toRoutesInput,
} from './edit.ts';
import type { LngLat } from './project.ts';

/** The routes `addRoute` returned; fails the test with its reason when it refused. */
function ok(out: DemoRoute[] | string): DemoRoute[] {
  if (typeof out === 'string') throw new Error(out);
  return out;
}

const two: DemoRoute[] = [
  {
    id: 'a',
    color: [1, 2, 3],
    points: [
      [0, 0],
      [1, 1],
    ],
  },
  {
    id: 'b',
    color: [4, 5, 6],
    points: [
      [2, 2],
      [3, 3],
      [4, 4],
    ],
  },
];

describe('the sample', () => {
  it('has three routes that share their first four points (a trunk), then split', () => {
    expect(SAMPLE_ROUTES.map((r) => r.id)).toEqual(['red', 'blue', 'yellow']);
    const trunk = SAMPLE_ROUTES[0]!.points.slice(0, 4);
    for (const r of SAMPLE_ROUTES) expect(r.points.slice(0, 4)).toEqual(trunk);
    const ends = SAMPLE_ROUTES.map((r) => JSON.stringify(r.points.at(-1)));
    expect(new Set(ends).size).toBe(3);
  });

  it('has a loop: yellow comes back to a corner it passed, then carries on', () => {
    const yellow = SAMPLE_ROUTES.find((r) => r.id === 'yellow')!;
    const corners = yellow.points.map((p) => JSON.stringify(p));
    const back = corners.findIndex((c, i) => corners.indexOf(c) < i);
    expect(back).toBeGreaterThan(0);
    expect(back).toBeLessThan(corners.length - 1);
  });
});

describe('addRoute', () => {
  it('adds a route with the next free id and color, copying the points', () => {
    const points: [number, number][] = [
      [5, 5],
      [6, 6],
    ];
    const out = addRoute(two, points);
    expect(out).not.toBeTypeOf('string');
    const added = ok(out)[2]!;
    expect(added).toEqual({
      id: 'route-1',
      color: PALETTE[2],
      points: [
        [5, 5],
        [6, 6],
      ],
    });
    points[0]![0] = 99;
    expect(added.points[0]).toEqual([5, 5]);
    const again = ok(
      addRoute(ok(out), [
        [7, 7],
        [8, 8],
      ]),
    );
    expect(again.map((r) => r.id)).toEqual(['a', 'b', 'route-1', 'route-2']);
    // A freed id is reused.
    const reused = ok(
      addRoute(removeRoute(again, 'route-1'), [
        [1, 2],
        [3, 4],
      ]),
    );
    expect(reused.at(-1)?.id).toBe('route-1');
  });

  it('wraps around the palette', () => {
    let routes: DemoRoute[] = [];
    for (let i = 0; i <= PALETTE.length; i++)
      routes = ok(
        addRoute(routes, [
          [i, 0],
          [i, 1],
        ]),
      );
    expect(routes.at(-1)?.color).toEqual(PALETTE[0]);
  });

  it.each<[LngLat[]]>([
    [[]],
    [[[1, 1]]],
    [
      [
        [1, 1],
        [1, 1],
        [1, 1],
      ],
    ],
  ])('refuses fewer than two different points: %j', (points) => {
    expect(addRoute(two, points)).toBe('A route needs at least two different points.');
  });
});

describe('removeRoute and movePoint', () => {
  it('removes one route, and nothing for an unknown id', () => {
    expect(removeRoute(two, 'a').map((r) => r.id)).toEqual(['b']);
    expect(removeRoute(two, 'zzz')).toEqual(two);
    expect(removeRoute([], 'a')).toEqual([]);
  });

  it('moves one point of one route and leaves the input as it was', () => {
    const out = movePoint(two, 'b', 1, [9, 9]);
    expect(out[1]?.points).toEqual([
      [2, 2],
      [9, 9],
      [4, 4],
    ]);
    expect(out[0]).toBe(two[0]);
    expect(two[1]?.points[1]).toEqual([3, 3]);
  });

  it.each([
    ['zzz', 0],
    ['a', -1],
    ['a', 2],
    ['a', Number.NaN],
  ])('ignores a move of a point that does not exist: %s %d', (id, index) => {
    expect(movePoint(two, id, index, [9, 9])).toEqual(two);
  });
});

describe('toRoutesInput and probeOf', () => {
  it('gives sideBySide one LineString per route with its id and color', () => {
    expect(toRoutesInput(two)).toEqual({
      type: 'FeatureCollection',
      features: [
        {
          type: 'Feature',
          id: 'a',
          properties: { color: [1, 2, 3] },
          geometry: {
            type: 'LineString',
            coordinates: [
              [0, 0],
              [1, 1],
            ],
          },
        },
        {
          type: 'Feature',
          id: 'b',
          properties: { color: [4, 5, 6] },
          geometry: {
            type: 'LineString',
            coordinates: [
              [2, 2],
              [3, 3],
              [4, 4],
            ],
          },
        },
      ],
    });
    expect(toRoutesInput([]).features).toEqual([]);
  });

  it('probes the middle of the first segment, or nothing', () => {
    expect(
      probeOf([
        [0, 0],
        [2, 4],
        [9, 9],
      ]),
    ).toEqual([1, 2]);
    expect(probeOf([[0, 0]])).toBeNull();
    expect(probeOf([])).toBeNull();
  });
});

describe('hexToRgb', () => {
  it('reads #rrggbb in either case', () => {
    expect(hexToRgb('#e4002b')).toEqual([228, 0, 43]);
    expect(hexToRgb('#00A3AD')).toEqual([0, 163, 173]);
  });

  it('gives null for anything else', () => {
    for (const bad of ['', '#fff', 'e4002b', '#e4002', '#e4002bff', '#zz0000', null, 7, {}]) {
      expect(hexToRgb(bad)).toBeNull();
    }
  });
});

const line = (id: unknown, coordinates: unknown, properties: unknown = {}) => ({
  type: 'Feature',
  id,
  properties,
  geometry: { type: 'LineString', coordinates },
});

describe('routesFromGeoJson', () => {
  it('reads each LineString with an id, its name and color', () => {
    const routes = routesFromGeoJson({
      type: 'FeatureCollection',
      features: [
        line(
          '16061-2',
          [
            [34.8, 32.0],
            [34.81, 32.01],
          ],
          { name: '61', color: '#e4002b' },
        ),
        line(7, [
          [1, 2],
          [3, 4],
        ]),
      ],
    });
    expect(routes).toEqual([
      {
        id: '16061-2',
        name: '61',
        color: [228, 0, 43],
        points: [
          [34.8, 32.0],
          [34.81, 32.01],
        ],
      },
      {
        id: '7',
        color: PALETTE[1],
        points: [
          [1, 2],
          [3, 4],
        ],
      },
    ]);
  });

  it('skips features without an id, other geometries, and lines with fewer than two points', () => {
    const routes = routesFromGeoJson({
      type: 'FeatureCollection',
      features: [
        line(undefined, [
          [0, 0],
          [1, 1],
        ]),
        line('one-point', [[0, 0]]),
        line('junk-points', [[0, 0], 'x', [1], null]),
        {
          type: 'Feature',
          id: 'p',
          properties: {},
          geometry: { type: 'Point', coordinates: [0, 0] },
        },
        null,
        'not a feature',
        line('ok', [
          [0, 0],
          [1, 1],
        ]),
      ],
    });
    expect(routes.map((r) => r.id)).toEqual(['ok']);
  });

  it('gives no routes for input that is not a FeatureCollection', () => {
    for (const bad of [null, undefined, 3, 'routes', [], {}, { features: 'no' }]) {
      expect(routesFromGeoJson(bad)).toEqual([]);
    }
  });

  it('feeds the Tel Aviv file to toRoutesInput with every route kept', () => {
    const text = readFileSync(
      join(import.meta.dirname, '..', 'public', 'data', 'tel-aviv-routes.geojson'),
      'utf8',
    );
    const routes = routesFromGeoJson(JSON.parse(text));
    expect(routes).toHaveLength(25);
    expect(new Set(routes.map((r) => r.id)).size).toBe(25);
    expect(toRoutesInput(routes).features).toHaveLength(25);
  });
});
