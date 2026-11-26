import { describe, expect, it } from 'vitest';

import { marksAlong, metersPerPixel } from './chevrons.ts';
import type { LngLat } from './project.ts';

// About 1 km east of 0°, 0° (111.32 m per 0.001° of longitude at the equator).
const east: LngLat[] = [
  [0, 0],
  [0.008983, 0],
];

describe('metersPerPixel', () => {
  it('is the 512-pixel tile scale: about 78 km per pixel at zoom 0 on the equator', () => {
    expect(metersPerPixel(0, 0)).toBeCloseTo(78_271.52, 1);
  });

  it('halves with each zoom step and shrinks with latitude', () => {
    expect(metersPerPixel(15, 0)).toBeCloseTo(metersPerPixel(14, 0) / 2, 9);
    expect(metersPerPixel(14, 60)).toBeCloseTo(metersPerPixel(14, 0) / 2, 6);
  });
});

describe('marksAlong', () => {
  it('spaces marks evenly, the first half a spacing in, all pointing the way the path runs', () => {
    const marks = marksAlong(east, 200);
    expect(marks).toHaveLength(5);
    const meters = marks.map((m) => Math.round((m.position[0] / 0.008983) * 1000));
    expect(meters).toEqual([100, 300, 500, 700, 900]);
    for (const m of marks) expect(m.angle).toBe(0);
  });

  it('turns each mark the way its segment runs: north 90°, west 180°, south -90°', () => {
    const north = marksAlong(
      [
        [0, 0],
        [0, 0.01],
      ],
      300,
    );
    const west = marksAlong(
      [
        [0, 0],
        [-0.01, 0],
      ],
      300,
    );
    const south = marksAlong(
      [
        [0, 0],
        [0, -0.01],
      ],
      300,
    );
    expect(north[0]?.angle).toBeCloseTo(90, 9);
    expect(west[0]?.angle).toBeCloseTo(180, 9);
    expect(south[0]?.angle).toBeCloseTo(-90, 9);
  });

  it('carries the spacing across a bend and skips repeated points', () => {
    const bend: LngLat[] = [
      [0, 0],
      [0.002695, 0], // 300 m east
      [0.002695, 0],
      [0.002695, 0.002714], // then 300 m north
    ];
    // Marks at 100 m and 300 m (the corner, still on the east segment), then 500 m north.
    const marks = marksAlong(bend, 200);
    expect(marks.map((m) => Math.round(m.angle))).toEqual([0, 0, 90]);
  });

  it('gives nothing for a path without a segment or a spacing that is not a positive number', () => {
    expect(marksAlong([], 100)).toEqual([]);
    expect(marksAlong([[0, 0]], 100)).toEqual([]);
    expect(
      marksAlong(
        [
          [1, 1],
          [1, 1],
        ],
        100,
      ),
    ).toEqual([]);
    for (const spacing of [0, -5, Number.NaN, Number.POSITIVE_INFINITY]) {
      expect(marksAlong(east, spacing)).toEqual([]);
    }
  });
});
