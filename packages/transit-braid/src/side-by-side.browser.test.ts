// sideBySide() end to end in a real browser (Chromium): GeoJSON → worker → WASM → GeoJSON.
import { describe, expect, it } from 'vitest';

import gridFixture from '../../../fixtures/stress/stress-grid.geojson?raw';
import yFixture from '../../../fixtures/synthetic/y-split.geojson?raw';
import { InputError } from './errors.ts';
import type { RoutesInput } from './routes.ts';
import { createSession, sideBySide } from './side-by-side.ts';

/** Tel Aviv, so the fixture's meters become real coordinates. */
const LNG0 = 34.78;
const LAT0 = 32.07;
const M_PER_DEG_LAT = 111_320;
const M_PER_DEG_LNG = M_PER_DEG_LAT * Math.cos((LAT0 * Math.PI) / 180);

interface MeterFixture {
  features: {
    properties: { line: string; color: string };
    geometry: { coordinates: number[][] };
  }[];
}

/** A synthetic fixture (meters) as routes in `[lng, lat]`, with `properties.line` as the id. */
function routesFrom(text: string): RoutesInput {
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- a fixture file, checked by the fixtures crate
  const fixture = JSON.parse(text) as MeterFixture;
  return {
    type: 'FeatureCollection',
    features: fixture.features.map((f) => ({
      type: 'Feature',
      id: f.properties.line,
      properties: f.properties,
      geometry: {
        type: 'LineString',
        coordinates: f.geometry.coordinates.map(([x = 0, y = 0]) => [
          LNG0 + x / M_PER_DEG_LNG,
          LAT0 + y / M_PER_DEG_LAT,
        ]),
      },
    })),
  };
}

/** Meters per degree where hand-made fixtures are placed, around 0°, 0°. */
const PLACE_M_PER_DEG = 111_319.490_793_273_57;

/** The stress grid (meters, placed around 0°, 0°) as routes; `moved`: its first route's middle
 * point moved about 12 m. */
function gridFrom(text: string, moved = false): RoutesInput {
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- a fixture file, checked by the fixtures crate
  const fixture = JSON.parse(text) as MeterFixture;
  return {
    type: 'FeatureCollection',
    features: fixture.features.map((f, i) => {
      const coordinates = f.geometry.coordinates.map(([x = 0, y = 0]) => [
        x / PLACE_M_PER_DEG,
        y / PLACE_M_PER_DEG,
      ]);
      const k = Math.floor(coordinates.length / 2);
      const [x = 0, y = 0] = coordinates[k] ?? [];
      if (moved && i === 0) coordinates[k] = [x + 0.0001, y + 0.000_05];
      return {
        type: 'Feature',
        id: f.properties.line,
        properties: null,
        geometry: { type: 'LineString', coordinates },
      };
    }),
  };
}

/** A street running east from the center for `m` meters. */
const east = (id: string | number, m = 400): RoutesInput['features'][number] => ({
  type: 'Feature',
  id,
  properties: null,
  geometry: {
    type: 'LineString',
    coordinates: [
      [LNG0, LAT0],
      [LNG0 + m / M_PER_DEG_LNG, LAT0],
    ],
  },
});

/** What a fresh call draws: a new session's first call starts from an empty cache. */
async function freshDraw(routes: RoutesInput, zoom: number, maxSideBySide?: number) {
  const session = createSession();
  try {
    return await session.sideBySide(routes, { zoom, maxSideBySide });
  } finally {
    session.close();
  }
}

describe('sideBySide in a real browser', () => {
  it('turns a fixture into one valid line per route, ids and properties kept', async () => {
    const routes = routesFrom(yFixture);
    const out = await sideBySide(routes, { zoom: 16 });
    expect(out.lines.type).toBe('FeatureCollection');
    expect(out.lines.features.map((f) => f.id)).toEqual(routes.features.map((f) => f.id));
    for (const [i, line] of out.lines.features.entries()) {
      expect(line.type).toBe('Feature');
      expect(line.properties).toEqual(routes.features[i]?.properties);
      expect(line.geometry.type).toBe('LineString');
      expect(line.geometry.coordinates.length).toBeGreaterThanOrEqual(2);
      for (const p of line.geometry.coordinates) {
        expect(p).toHaveLength(2);
        expect(p.every(Number.isFinite)).toBe(true);
      }
    }
    expect(out.stations).toEqual({ type: 'FeatureCollection', features: [] });
  });

  it('puts two routes on one street one pitch apart', async () => {
    // 1 m per pixel, line 4 px + gap 1 px: 5 m between the two lines.
    const out = await sideBySide(
      { type: 'FeatureCollection', features: [east('A'), east('B')] },
      { metersPerPixel: 1 },
    );
    const [a, b] = out.lines.features.map((f) => f.geometry.coordinates[0]?.[1] ?? NaN);
    expect(Math.abs((a ?? NaN) - (b ?? NaN)) * M_PER_DEG_LAT).toBeCloseTo(5, 1);
  });

  it('leaves out routes over a lane cap through the worker, and lists them', async () => {
    const three = {
      type: 'FeatureCollection' as const,
      features: [east('A'), east('B'), east('C')],
    };
    const capped = await sideBySide(three, { zoom: 16, maxSideBySide: 2 });
    expect(capped.dropped).toEqual(['C']);
    expect(capped.lines.features.map((f) => f.id)).toEqual(['A', 'B']);
    const alone = await sideBySide(
      { type: 'FeatureCollection', features: [east('A'), east('B')] },
      { zoom: 16 },
    );
    expect(capped.lines).toEqual(alone.lines);
    const uncapped = await sideBySide(three, { zoom: 16 });
    expect(uncapped.dropped).toEqual([]);
    expect(uncapped.lines.features).toHaveLength(3);
  });

  it('keeps each route on its own line in a session when routes trade places', async () => {
    // Two copies of one street: the same points, two ids. Listed the other way round, each id
    // keeps the line it had (matched by id, not by place in the list).
    const session = createSession();
    const first = await session.sideBySide(
      { type: 'FeatureCollection', features: [east('twin-a'), east('twin-b')] },
      { metersPerPixel: 1 },
    );
    const second = await session.sideBySide(
      { type: 'FeatureCollection', features: [east('twin-b'), east('twin-a')] },
      { metersPerPixel: 1 },
    );
    session.close();
    const lineOf = (out: typeof first, id: string) =>
      out.lines.features.find((f) => f.id === id)?.geometry.coordinates;
    expect(lineOf(second, 'twin-a')).toEqual(lineOf(first, 'twin-a'));
    expect(lineOf(second, 'twin-b')).toEqual(lineOf(first, 'twin-b'));
    expect(lineOf(first, 'twin-a')).not.toEqual(lineOf(first, 'twin-b'));
  });

  it('is pure: the same input draws the same way after other calls; a session remembers', async () => {
    // The stress grid's edit, drawn warm from the grid before it, differs from a fresh draw.
    const grid = gridFrom(gridFixture);
    const moved = gridFrom(gridFixture, true);
    const fresh = await sideBySide(moved, { zoom: 15 });
    await sideBySide(grid, { zoom: 15 });
    expect(await sideBySide(moved, { zoom: 15 })).toEqual(fresh);

    const session = createSession();
    await session.sideBySide(grid, { zoom: 15 });
    const warm = await session.sideBySide(moved, { zoom: 15 });
    expect(warm).not.toEqual(fresh);
    // A zoom change in the session: the same lines as a fresh draw of that zoom would give after
    // the same edit in a new session.
    const zoomed = await session.sideBySide(moved, { zoom: 17 });
    session.close();
    await expect(session.sideBySide(moved, { zoom: 15 })).rejects.toMatchObject({
      code: 'CLOSED',
    });
    const again = createSession();
    await again.sideBySide(grid, { zoom: 15 });
    await again.sideBySide(moved, { zoom: 15 });
    expect(await again.sideBySide(moved, { zoom: 17 })).toEqual(zoomed);
    again.close();
  });

  it('redraws a zoom change fast with the bits a fresh call draws, and an edit from scratch', async () => {
    const grid = gridFrom(gridFixture);
    const moved = gridFrom(gridFixture, true);
    await sideBySide(moved, { zoom: 15 });
    let start = performance.now();
    const drawn = await sideBySide(grid, { zoom: 15 });
    const first = performance.now() - start;
    expect(drawn).toEqual(await freshDraw(grid, 15));
    start = performance.now();
    const zoomed = await sideBySide(grid, { zoom: 17.75 });
    const zoom = performance.now() - start;
    expect(zoomed).toEqual(await freshDraw(grid, 17.75));
    // A first draw of the grid takes about 140 ms, a zoom change about 1 ms.
    expect(zoom).toBeLessThan(first / 4);
    // An edit and a cap: from scratch, not from the call before.
    expect(await sideBySide(moved, { zoom: 17.75 })).toEqual(await freshDraw(moved, 17.75));
    expect(await sideBySide(moved, { zoom: 16, maxSideBySide: 2 })).toEqual(
      await freshDraw(moved, 16, 2),
    );
    expect(await sideBySide(moved, { zoom: 12 })).toEqual(await freshDraw(moved, 12));
  });

  it('answers many calls at once, each with its own result', async () => {
    const calls = [1, 2, 3, 4].map((n) =>
      sideBySide(
        { type: 'FeatureCollection', features: Array.from({ length: n }, (_, i) => east(i)) },
        { zoom: 15 },
      ),
    );
    const results = await Promise.all(calls);
    expect(results.map((r) => r.lines.features.length)).toEqual([1, 2, 3, 4]);
  });

  it('rejects 71 routes with a clear error', async () => {
    const routes: RoutesInput = {
      type: 'FeatureCollection',
      features: Array.from({ length: 71 }, (_, i) => east(i)),
    };
    await expect(sideBySide(routes, { zoom: 15 })).rejects.toMatchObject({
      code: 'TOO_MANY_ROUTES',
      message: 'Too many routes: 71 given, at most 70 allowed.',
    });
  });

  it('names the route when the core refuses it', async () => {
    const offMap = east('north');
    const routes: RoutesInput = {
      type: 'FeatureCollection',
      features: [
        east('ok'),
        {
          ...offMap,
          geometry: {
            type: 'LineString',
            coordinates: [
              [0, 0],
              [0, 89],
            ],
          },
        },
      ],
    };
    const call = sideBySide(routes, { zoom: 15 });
    await expect(call).rejects.toThrow(InputError);
    await expect(sideBySide(routes, { zoom: 15 })).rejects.toThrow(/^Route "north", point 1:/);
    await expect(sideBySide(routes, { zoom: 15 })).rejects.toMatchObject({
      code: 'BAD_COORDINATE',
      routeId: 'north',
      index: 1,
      point: 1,
    });
    const short: RoutesInput = {
      type: 'FeatureCollection',
      features: [
        {
          ...east(3),
          geometry: {
            type: 'LineString',
            coordinates: [
              [1, 1],
              [1, 1],
            ],
          },
        },
      ],
    };
    await expect(sideBySide(short, { zoom: 15 })).rejects.toMatchObject({
      code: 'TOO_FEW_POINTS',
      routeId: 3,
      index: 0,
      message: 'Route 3: needs at least two distinct points',
    });
  });

  it('draws a bar across the bundle for a station on the street, none for one far away', async () => {
    const station = (id: string, northM: number) => ({
      type: 'Feature' as const,
      id,
      properties: { name: id },
      geometry: {
        type: 'Point' as const,
        coordinates: [LNG0 + 200 / M_PER_DEG_LNG, LAT0 + northM / M_PER_DEG_LAT],
      },
    });
    const out = await sideBySide(
      { type: 'FeatureCollection', features: [east('A'), east('B')] },
      {
        metersPerPixel: 1,
        stations: { type: 'FeatureCollection', features: [station('on', 2), station('far', 300)] },
      },
    );
    expect(out.stations.features.map((f) => f.id)).toEqual(['on']);
    const bar = out.stations.features[0]!;
    expect(bar.properties).toEqual({ name: 'on' });
    const [[x1 = 0, y1 = 0] = [], [x2 = 0, y2 = 0] = []] = bar.geometry.coordinates;
    // Square to the street, across both lines: 2 × (half a pitch + half a line) = 9 m.
    expect(Math.abs(x1 - x2) * M_PER_DEG_LNG).toBeLessThan(0.01);
    expect(Math.abs(y1 - y2) * M_PER_DEG_LAT).toBeCloseTo(9, 1);
  });

  it('gives same-name stops close together one shared bar, keeping each station', async () => {
    const stop = (id: string, name: string, eastM: number, northM: number) => ({
      type: 'Feature' as const,
      id,
      properties: { name },
      geometry: {
        type: 'Point' as const,
        coordinates: [LNG0 + eastM / M_PER_DEG_LNG, LAT0 + northM / M_PER_DEG_LAT],
      },
    });
    const out = await sideBySide(
      { type: 'FeatureCollection', features: [east('A'), east('B')] },
      {
        metersPerPixel: 1,
        stations: {
          type: 'FeatureCollection',
          features: [
            stop('n', 'Main', 190, 8),
            stop('s', 'Main', 210, -8),
            stop('x', 'Park', 300, 6),
          ],
        },
      },
    );
    expect(out.stations.features.map((f) => f.id)).toEqual(['n', 's', 'x']);
    const [n, s, x] = out.stations.features.map((f) => f.geometry.coordinates);
    expect(n).toEqual(s);
    expect(((n?.[0]?.[0] ?? 0) - LNG0) * M_PER_DEG_LNG).toBeCloseTo(200, 3);
    expect(((x?.[0]?.[0] ?? 0) - LNG0) * M_PER_DEG_LNG).toBeCloseTo(300, 3);
    expect(out.stations.features[1]?.properties).toEqual({ name: 'Main' });
  });

  it('names a station the core refuses by its id', async () => {
    const bad = {
      type: 'FeatureCollection' as const,
      features: [
        {
          type: 'Feature' as const,
          id: 'pole',
          properties: null,
          geometry: { type: 'Point' as const, coordinates: [0, 89] },
        },
      ],
    };
    await expect(
      sideBySide({ type: 'FeatureCollection', features: [east('A')] }, { zoom: 15, stations: bad }),
    ).rejects.toMatchObject({
      code: 'BAD_STATION',
      stationId: 'pole',
      index: 0,
      message: expect.stringMatching(/^Station "pole": needs a finite longitude/),
    });
  });

  it('keeps working after a refused call', async () => {
    await expect(
      sideBySide(
        {
          type: 'FeatureCollection',
          features: [
            {
              ...east('x'),
              geometry: {
                type: 'LineString',
                coordinates: [
                  [0, 0],
                  [0, 0],
                ],
              },
            },
          ],
        },
        { zoom: 15 },
      ),
    ).rejects.toThrow(InputError);
    const out = await sideBySide(
      { type: 'FeatureCollection', features: [east('y')] },
      { zoom: 15 },
    );
    expect(out.lines.features[0]?.id).toBe('y');
  });
});
