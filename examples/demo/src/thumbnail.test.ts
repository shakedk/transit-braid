import { describe, expect, it } from 'vitest';

import type { LngLat } from './project.ts';
import { fitPaths } from './thumbnail.ts';

const near = (a: number, b: number) => expect(a).toBeCloseTo(b, 6);

describe('fitPaths', () => {
  it('fills the wider side of the box, minus the padding, and centers the other', () => {
    // 2 units wide, 1 tall around the equator, into 220 x 120 with 10 px padding: scale 100.
    const [line] = fitPaths(
      [
        [
          [0, -0.5],
          [2, 0.5],
        ],
      ],
      220,
      120,
      10,
    );
    expect(line).toHaveLength(2);
    near(line?.[0]?.[0] ?? Number.NaN, 10);
    near(line?.[0]?.[1] ?? Number.NaN, 110);
    near(line?.[1]?.[0] ?? Number.NaN, 210);
    near(line?.[1]?.[1] ?? Number.NaN, 10);
  });

  it('puts north up: a point further north is higher in the picture', () => {
    const [line] = fitPaths(
      [
        [
          [0, 0],
          [0, 1],
        ],
      ],
      100,
      100,
      0,
    );
    expect(line?.[1]?.[1]).toBeLessThan(line?.[0]?.[1] ?? 0);
  });

  it('keeps shapes: a degree of longitude is shorter away from the equator', () => {
    // A 1° x 1° square at 60° N is twice as tall as wide.
    const square: LngLat[] = [
      [0, 59.5],
      [1, 59.5],
      [1, 60.5],
      [0, 60.5],
    ];
    const [pts] = fitPaths([square], 400, 400, 0);
    const xs = (pts ?? []).map((p) => p[0]);
    const ys = (pts ?? []).map((p) => p[1]);
    const w = Math.max(...xs) - Math.min(...xs);
    const h = Math.max(...ys) - Math.min(...ys);
    expect(h / w).toBeCloseTo(1 / Math.cos((60 * Math.PI) / 180), 2);
  });

  it('puts a single point, or several on one spot, at the center', () => {
    for (const paths of [[[[5, 5]]], [[[5, 5]], [[5, 5]]]] as LngLat[][][]) {
      for (const path of fitPaths(paths, 100, 60, 8)) {
        for (const p of path) expect(p).toEqual([50, 30]);
      }
    }
  });

  it('gives empty paths back for no points', () => {
    expect(fitPaths([], 100, 100, 5)).toEqual([]);
    expect(fitPaths([[], []], 100, 100, 5)).toEqual([[], []]);
  });
});
