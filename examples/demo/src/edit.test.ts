import { describe, expect, it } from 'vitest';

import {
  addRoute,
  type DemoRoute,
  movePoint,
  PALETTE,
  probeOf,
  removeRoute,
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
  it('has three routes that share their first three points (a trunk), then split', () => {
    expect(SAMPLE_ROUTES.map((r) => r.id)).toEqual(['red', 'blue', 'yellow']);
    const trunk = SAMPLE_ROUTES[0]!.points.slice(0, 3);
    for (const r of SAMPLE_ROUTES) expect(r.points.slice(0, 3)).toEqual(trunk);
    const ends = SAMPLE_ROUTES.map((r) => JSON.stringify(r.points.at(-1)));
    expect(new Set(ends).size).toBe(3);
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
