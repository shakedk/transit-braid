import { readFileSync } from 'node:fs';
import { join } from 'node:path';

import { describe, expect, test } from 'vitest';

import { random, stressGrid } from './stress-grid.ts';

const committed = join(import.meta.dirname, '..', 'fixtures', 'stress', 'stress-grid.geojson');

interface Fixture {
  braid: { name: string; units: string; license: string };
  features: {
    properties: { line: string; color: string };
    geometry: { type: string; coordinates: number[][] };
  }[];
}

// oxlint-disable-next-line typescript/no-unsafe-type-assertion -- our own generator's output
const parse = (text: string) => JSON.parse(text) as Fixture;

describe('stress grid', () => {
  test('the committed fixture is exactly what the script writes', () => {
    expect(parse(readFileSync(committed, 'utf8'))).toEqual(parse(stressGrid()));
  });

  test('the same seed gives the same numbers', () => {
    const a = random(7);
    const b = random(7);
    const first = Array.from({ length: 5 }, a);
    expect(Array.from({ length: 5 }, b)).toEqual(first);
    expect(first.every((v) => v >= 0 && v < 1)).toBe(true);
    expect(Array.from({ length: 5 }, random(8))).not.toEqual(first);
  });

  test('70 routes, the most one call takes, each a street walk inside the grid', () => {
    const f = parse(stressGrid());
    expect(f.braid).toMatchObject({ name: 'stress-grid', units: 'meters', license: 'MIT' });
    expect(f.features).toHaveLength(70);
    expect(new Set(f.features.map((x) => x.properties.line)).size).toBe(70);
    const problems: string[] = [];
    for (const { geometry, properties } of f.features) {
      const c = geometry.coordinates;
      const id = properties.line;
      if (geometry.type !== 'LineString' || c.length < 2) problems.push(`${id}: not a line`);
      // Jittered by at most 1.5 m each way off the 200 m grid, inside its 2.2 km square.
      for (const v of c.flat()) {
        const offGrid = Math.abs(v - Math.round(v / 200) * 200);
        if (offGrid > 1.5 || v < -1.5 || v > 2201.5)
          problems.push(`${id}: ${String(v)} off the grid`);
      }
      // Along streets only: every leg is straight east-west or north-south.
      for (let i = 1; i < c.length; i++) {
        const [ax = 0, ay = 0] = c[i - 1] ?? [];
        const [bx = 0, by = 0] = c[i] ?? [];
        if (ax !== bx && ay !== by) problems.push(`${id}: leg ${String(i)} is diagonal`);
      }
    }
    expect(problems).toEqual([]);
  });
});
