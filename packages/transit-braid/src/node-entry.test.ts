// transit-braid/node: the same sideBySide() in Node 24+, on the caller's thread, with
// the real WASM read from disk (`pnpm build:wasm` first), and createBraidWorker() on a
// worker_threads thread. Two tests run it in a plain `node` process through the package's
// `./node` export, as an app would.
import { execFile } from 'node:child_process';
import { join } from 'node:path';
import { promisify } from 'node:util';

import { afterEach, describe, expect, it } from 'vitest';

import {
  type BraidWorker,
  createBraidWorker,
  InputError,
  type RoutesInput,
  sideBySide,
  type SideBySideOptions,
} from './node.ts';

const LAT0 = 32.07;
const LNG0 = 34.78;
/** Meters per degree near Tel Aviv (only for reading the answers back). */
const M_PER_DEG_LAT = 110_900;
const M_PER_DEG_LNG = 94_300;

/** A route `id` running 500 m east, `northM` meters north of the street. */
function east(id: string | number, northM = 0): RoutesInput['features'][number] {
  const lat = LAT0 + northM / M_PER_DEG_LAT;
  return {
    type: 'Feature',
    id,
    properties: { name: `route ${String(id)}` },
    geometry: {
      type: 'LineString',
      coordinates: [
        [LNG0, lat],
        [LNG0 + 500 / M_PER_DEG_LNG, lat],
      ],
    },
  };
}

const routes = (...features: RoutesInput['features']): RoutesInput => ({
  type: 'FeatureCollection',
  features,
});

describe('transit-braid/node', () => {
  it('draws two routes on one street side by side, ids and properties kept', async () => {
    const out = await sideBySide(routes(east('A'), east('B')), { zoom: 16 });
    expect(out.lines.features.map((f) => f.id)).toEqual(['A', 'B']);
    expect(out.lines.features.map((f) => f.properties)).toEqual([
      { name: 'route A' },
      { name: 'route B' },
    ]);
    const [a = Number.NaN, b = Number.NaN] = out.lines.features.map(
      (f) => f.geometry.coordinates[0]?.[1] ?? Number.NaN,
    );
    // One street, two lanes: one to each side of it, the same distance away.
    expect(a - LAT0).toBeCloseTo(-(b - LAT0), 12);
    expect(Math.abs(a - b)).toBeGreaterThan(0);
    expect(out.stations).toEqual({ type: 'FeatureCollection', features: [] });
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
    const out = await sideBySide(routes(east('A'), east('B')), {
      metersPerPixel: 1,
      stations: { type: 'FeatureCollection', features: [station('on', 2), station('far', 300)] },
    });
    expect(out.stations.features.map((f) => f.id)).toEqual(['on']);
    expect(out.stations.features[0]?.properties).toEqual({ name: 'on' });
    const [[, y1 = 0] = [], [, y2 = 0] = []] = out.stations.features[0]?.geometry.coordinates ?? [];
    // Across both lines: 2 × (half a pitch + half a line) = 9 m.
    expect(Math.abs(y1 - y2) * M_PER_DEG_LAT).toBeCloseTo(9, 1);
  });

  it('gives an empty result for no routes', async () => {
    const out = await sideBySide(routes(), { zoom: 16 });
    expect(out).toEqual({
      lines: { type: 'FeatureCollection', features: [] },
      stations: { type: 'FeatureCollection', features: [] },
      dropped: [],
    });
  });

  it('answers many calls at once, each with its own result, as one call alone would', async () => {
    const many = await Promise.all(
      [1, 2, 3, 4].map((n) =>
        sideBySide(routes(...Array.from({ length: n }, (_, i) => east(i))), { zoom: 15 }),
      ),
    );
    expect(many.map((r) => r.lines.features.length)).toEqual([1, 2, 3, 4]);
    const alone = await sideBySide(routes(east(0), east(1), east(2)), { zoom: 15 });
    expect(many[2]).toEqual(alone);
  });

  it.each([
    ['null routes', null, { zoom: 16 }, 'BAD_ROUTES', 'must be a GeoJSON FeatureCollection'],
    ['null options', routes(east('A')), null, 'BAD_OPTION', '`options` must be an object'],
    ['no zoom or scale', routes(east('A')), {}, 'BAD_OPTION', 'exactly one of `zoom`'],
    [
      'a route without an id',
      { type: 'FeatureCollection', features: [{ ...east('A'), id: undefined }] },
      { zoom: 16 },
      'MISSING_ID',
      'Feature 0 needs an `id`',
    ],
    [
      'a point that is not a number',
      {
        type: 'FeatureCollection',
        features: [{ ...east('A'), geometry: { type: 'LineString', coordinates: [[LNG0, null]] } }],
      },
      { zoom: 16 },
      'BAD_COORDINATE',
      'Route "A", point 0',
    ],
  ] as const)('refuses %s with a named error', async (_, input, options, code, message) => {
    const call = sideBySide(
      // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- deliberately wrong input
      input as unknown as RoutesInput,
      // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- deliberately wrong input
      options as unknown as SideBySideOptions,
    );
    await expect(call).rejects.toBeInstanceOf(InputError);
    await expect(call).rejects.toMatchObject({ code, message: expect.stringContaining(message) });
  });

  it('names the route the WASM refuses, and 71 routes are too many', async () => {
    const north = { ...east('north'), geometry: { ...east('north').geometry } };
    north.geometry.coordinates = [
      [LNG0, 95],
      [LNG0 + 0.01, 95],
    ];
    const offMap = sideBySide(routes(east('ok'), north), { zoom: 16 });
    await expect(offMap).rejects.toBeInstanceOf(InputError);
    await expect(offMap).rejects.toMatchObject({
      code: 'BAD_COORDINATE',
      routeId: 'north',
      index: 1,
      point: 0,
      message:
        'Route "north", point 0: needs a finite longitude from -180 to 180 and latitude from -85.05 to 85.05',
    });
    const tooMany = routes(...Array.from({ length: 71 }, (_, i) => east(i)));
    await expect(sideBySide(tooMany, { zoom: 15 })).rejects.toThrow(
      'Too many routes: 71 given, at most 70 allowed.',
    );
    // A refusal leaves it working.
    const after = await sideBySide(routes(east('A')), { zoom: 16 });
    expect(after.lines.features).toHaveLength(1);
  });

  it('leaves out later routes on a full street with a lane cap, the rest drawn alone', async () => {
    // A, B and C share a street; D runs alone 300 m north. Cap 2: C is left out.
    const all = routes(east('A'), east('B'), east('C'), east('D', 300));
    const capped = await sideBySide(all, { zoom: 16, maxSideBySide: 2 });
    expect(capped.dropped).toEqual(['C']);
    expect(capped.lines.features.map((f) => f.id)).toEqual(['A', 'B', 'D']);
    const alone = await sideBySide(routes(east('A'), east('B'), east('D', 300)), { zoom: 16 });
    expect(capped.lines).toEqual(alone.lines);
    // The same routes at every zoom; room for all (or no cap) leaves none out.
    const zooms = await Promise.all(
      [11, 18.5].map(async (zoom) => sideBySide(all, { zoom, maxSideBySide: 2 })),
    );
    expect(zooms.map((out) => out.dropped)).toEqual([['C'], ['C']]);
    const roomy = await Promise.all(
      [3, 50, null, undefined].map(async (maxSideBySide) =>
        sideBySide(all, { zoom: 16, maxSideBySide }),
      ),
    );
    expect(roomy.map((out) => out.dropped)).toEqual([[], [], [], []]);
    expect(roomy.map((out) => out.lines.features.length)).toEqual([4, 4, 4, 4]);
    // Cap 1: only A on the shared street; D, alone on its street, stays.
    expect((await sideBySide(all, { zoom: 16, maxSideBySide: 1 })).dropped).toEqual(['B', 'C']);
  });

  it('refuses a bad lane cap, and bad routes even when the cap would leave them out', async () => {
    const all = routes(east('A'), east('B'));
    // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- deliberately wrong input
    const text = '2' as unknown as number;
    const refusals = [0, -1, 1.5, Number.NaN, text].map(async (maxSideBySide) =>
      sideBySide(all, { zoom: 16, maxSideBySide }).then(
        () => 'drawn',
        (error: unknown) =>
          error instanceof InputError ? `${error.code} ${String(error.option)}` : String(error),
      ),
    );
    expect(await Promise.all(refusals)).toEqual(
      Array.from({ length: 5 }, () => 'BAD_OPTION maxSideBySide'),
    );
    const bad = { ...east('bad'), geometry: { ...east('bad').geometry } };
    bad.geometry.coordinates = [
      [LNG0, 95],
      [LNG0 + 0.01, 95],
    ];
    const refused = sideBySide(routes(east('A'), east('B'), bad), { zoom: 16, maxSideBySide: 1 });
    await expect(refused).rejects.toBeInstanceOf(InputError);
    await expect(refused).rejects.toMatchObject({
      code: 'BAD_COORDINATE',
      routeId: 'bad',
      index: 2,
    });
    const empty = await sideBySide(routes(), { zoom: 16, maxSideBySide: 1 });
    expect(empty.dropped).toEqual([]);
  });

  it('runs in a plain node process through the package export, with the same bits', async () => {
    const script = `
      import { sideBySide } from 'transit-braid/node';
      const input = JSON.parse(process.argv[1]);
      const out = await sideBySide(input, { zoom: 16 });
      process.stdout.write(JSON.stringify(out));
    `;
    const input = routes(east('A'), east('B'), east('C', 3));
    // The demo package depends on transit-braid, so `transit-braid/node` resolves from there.
    const { stdout } = await promisify(execFile)(
      process.execPath,
      ['--input-type=module', '-e', script, JSON.stringify(input)],
      { cwd: join(import.meta.dirname, '..', '..', '..', 'examples', 'demo') },
    );
    expect(JSON.parse(stdout)).toEqual(await sideBySide(input, { zoom: 16 }));
  });
});

/** A route `id` through `coordinates` (`[lng, lat]`). */
function through(id: string, coordinates: number[][]): RoutesInput['features'][number] {
  return { type: 'Feature', id, properties: {}, geometry: { type: 'LineString', coordinates } };
}

/** The call's error, and how long it took to come back. */
async function refusal(input: RoutesInput): Promise<{ error: unknown; ms: number }> {
  const started = performance.now();
  const error = await sideBySide(input, { zoom: 15 }).then(
    () => undefined,
    (e: unknown) => e,
  );
  return { error, ms: performance.now() - started };
}

/** Two routes drawn, so a refusal is known to leave the WASM working. */
async function stillWorks(): Promise<void> {
  const after = await sideBySide(routes(east('A'), east('B')), { zoom: 16 });
  expect(after.lines.features).toHaveLength(2);
}

/** A route `id` going round a 10 m circle 100 times (2000 points, 20 a lap). */
function knot(id: string, phase: number): RoutesInput['features'][number] {
  return through(
    id,
    Array.from({ length: 2000 }, (_, k) => {
      const a = ((k + phase) * 2 * Math.PI) / 20;
      return [LNG0 + (10 * Math.cos(a)) / M_PER_DEG_LNG, LAT0 + (10 * Math.sin(a)) / M_PER_DEG_LAT];
    }),
  );
}

describe('transit-braid/node input limits and work budget', () => {
  it('refuses routes around the globe at once, and keeps working', async () => {
    // Without the limits this ran out of memory in WASM after 3 s, and every later call failed.
    const globe = routes(
      through('west', [
        [-170, 0],
        [170, 0],
      ]),
      through('north', [
        [-170, 1],
        [170, 1],
      ]),
    );
    const { error, ms } = await refusal(globe);
    expect(error).toBeInstanceOf(InputError);
    expect(error).toMatchObject({
      code: 'CROSSES_ANTIMERIDIAN',
      routeId: 'west',
      index: 0,
      point: 1,
    });
    expect(String(error)).toContain('Route "west", point 1: more than 180° of longitude');
    expect(ms).toBeLessThan(1000);
    // With points in between the jumps are small, and the spread is refused instead.
    const around = routes(
      through('around', [
        [-170, 0],
        [-60, 0],
        [60, 0],
        [170, 0],
      ]),
    );
    const wide = await refusal(around);
    expect(wide.error).toMatchObject({ code: 'TOO_WIDE' });
    expect(String(wide.error)).toContain('at most 200 km each way');
    expect(wide.ms).toBeLessThan(1000);
    await stillWorks();
  });

  it('names the route that crosses the date line', async () => {
    const fiji = through('fiji', [
      [179.98, -16.8],
      [179.99, -16.8],
      [-179.99, -16.8],
    ]);
    await expect(sideBySide(routes(east('A'), fiji), { zoom: 15 })).rejects.toThrow(
      'Route "fiji", point 2: more than 180° of longitude from the point before, so it crosses the date line',
    );
  });

  it('refuses more than 2000 km of routes together', async () => {
    // Laps of 190 km east and west, 1 km apart, so it stays inside the 200 km box: 11 of them.
    const lap = 190_000 / M_PER_DEG_LNG;
    const laps = Array.from({ length: 12 }, (_, k) => [
      [LNG0 + (k % 2) * lap, LAT0 + (k * 1000) / M_PER_DEG_LAT],
      [LNG0 + ((k + 1) % 2) * lap, LAT0 + (k * 1000) / M_PER_DEG_LAT],
    ]).flat();
    const { error, ms } = await refusal(routes(through('long', laps)));
    expect(error).toBeInstanceOf(InputError);
    expect(error).toMatchObject({ code: 'TOO_LONG' });
    expect(String(error)).toMatch(/the routes are 2\d{3}\.\d km long together; at most 2000 km/);
    expect(ms).toBeLessThan(1000);
  });

  it('refuses points piled in one place fast, and keeps working', async () => {
    // Ten routes going round a 10 m circle 100 times each: 220 million nearby checks, about 3 s
    // natively without a bound. The work budget stops at 20 million.
    const knots = routes(...Array.from({ length: 10 }, (_, i) => knot(`knot ${i}`, i * 0.37)));
    const { error, ms } = await refusal(knots);
    expect(error).toBeInstanceOf(InputError);
    expect(error).toMatchObject({ code: 'TOO_COMPLEX' });
    expect(String(error)).toContain(
      'too complex: too many points lie close together (more than 20000000 nearby-segment checks)',
    );
    expect(ms).toBeLessThan(5000);
    await stillWorks();
  });
});

describe('transit-braid/node createBraidWorker (worker_threads)', () => {
  let worker: BraidWorker | undefined;
  afterEach(() => worker?.terminate());

  it('gives the same lines and bars as the call on this thread, to the bit', async () => {
    worker = createBraidWorker();
    const input = routes(east('A'), east('B'), east('C', 3));
    const stations = {
      type: 'FeatureCollection' as const,
      features: [
        {
          type: 'Feature' as const,
          id: 's',
          properties: null,
          geometry: { type: 'Point' as const, coordinates: [LNG0 + 0.002, LAT0] },
        },
      ],
    };
    const options = { zoom: 16, stations };
    const there = await worker.sideBySide(input, options);
    expect(there).toEqual(await sideBySide(input, options));
    expect(there.stations.features).toHaveLength(1);
  });

  it('answers many calls at once, each with its own result', async () => {
    worker = createBraidWorker();
    const w = worker;
    const many = await Promise.all(
      [1, 2, 3].map((n) =>
        w.sideBySide(routes(...Array.from({ length: n }, (_, i) => east(i))), { zoom: 15 }),
      ),
    );
    expect(many.map((r) => r.lines.features.length)).toEqual([1, 2, 3]);
  });

  it('refuses bad input with the same named errors, and keeps working', async () => {
    worker = createBraidWorker();
    // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- deliberately wrong input
    const noRoutes = worker.sideBySide(null as unknown as RoutesInput, { zoom: 16 });
    await expect(noRoutes).rejects.toBeInstanceOf(InputError);
    await expect(noRoutes).rejects.toMatchObject({
      code: 'BAD_ROUTES',
      message: '`routes` must be a GeoJSON FeatureCollection.',
    });
    await expect(worker.sideBySide(routes(east('A')), {})).rejects.toMatchObject({
      code: 'BAD_OPTION',
    });
    const north = { ...east('north'), geometry: { ...east('north').geometry } };
    north.geometry.coordinates = [
      [LNG0, 95],
      [LNG0 + 0.01, 95],
    ];
    await expect(worker.sideBySide(routes(east('ok'), north), { zoom: 16 })).rejects.toMatchObject({
      code: 'BAD_COORDINATE',
      routeId: 'north',
      index: 1,
      point: 0,
    });
    const after = await worker.sideBySide(routes(east('A')), { zoom: 16 });
    expect(after.lines.features).toHaveLength(1);
  });

  it('rejects waiting calls when terminated', async () => {
    worker = createBraidWorker();
    const call = worker.sideBySide(routes(east('A')), { zoom: 16 });
    worker.terminate();
    await expect(call).rejects.toMatchObject({
      name: 'BraidError',
      code: 'TERMINATED',
      message: 'Braid worker was terminated.',
    });
  });

  it('runs in a plain node process, and lets it exit without terminate()', async () => {
    const script = `
      import { createBraidWorker } from 'transit-braid/node';
      const out = await createBraidWorker().sideBySide(JSON.parse(process.argv[1]), { zoom: 16 });
      process.stdout.write(JSON.stringify(out));
    `;
    const input = routes(east('A'), east('B'));
    // A worker that kept the process alive would hit this timeout instead of exiting. The V8
    // flag is there because Node refused a worker given the parent's flags with one.
    const { stdout } = await promisify(execFile)(
      process.execPath,
      ['--max-old-space-size=4096', '--input-type=module', '-e', script, JSON.stringify(input)],
      { cwd: join(import.meta.dirname, '..', '..', '..', 'examples', 'demo'), timeout: 10_000 },
    );
    expect(JSON.parse(stdout)).toEqual(await sideBySide(input, { zoom: 16 }));
  });
});
