// One test per error code, through transit-braid/node and the real WASM (`pnpm build:wasm`
// first): bad input in, the code and details out. The tables are keyed by code, so the compiler
// fails this file when a code has no test. ENGINE_CRASHED and WORKER_FAILED need a crash or a dead
// worker: the crash projects test them (recovery.crash*.test.ts).
import { describe, expect, it } from 'vitest';

import {
  BraidError,
  InputError,
  type InputErrorCode,
  InternalError,
  type InternalErrorCode,
  internalError,
  asBraidError,
  Refused,
} from './errors.ts';
import { createBraidWorker, type RoutesInput, sideBySide, type SideBySideOptions } from './node.ts';
import { run } from './run.ts';

const LAT0 = 32.07;
const LNG0 = 34.78;
const M_PER_DEG_LAT = 110_900;
const M_PER_DEG_LNG = 94_300;

type Feature = RoutesInput['features'][number];

/** A route `id` through `coordinates` (`[lng, lat]`). */
function through(id: string | number, coordinates: number[][]): Feature {
  return { type: 'Feature', id, properties: null, geometry: { type: 'LineString', coordinates } };
}

/** A route `id` running 500 m east, `northM` meters north of the street. */
function east(id: string | number, northM = 0): Feature {
  const lat = LAT0 + northM / M_PER_DEG_LAT;
  return through(id, [
    [LNG0, lat],
    [LNG0 + 500 / M_PER_DEG_LNG, lat],
  ]);
}

const routes = (...features: unknown[]): RoutesInput =>
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- some tests pass bad features on purpose
  ({ type: 'FeatureCollection', features }) as unknown as RoutesInput;

/** A station `id` at `[lng, lat]`. */
const station = (id: string | undefined, coordinates: number[]) => ({
  type: 'Feature' as const,
  ...(id === undefined ? {} : { id }),
  properties: null,
  geometry: { type: 'Point' as const, coordinates },
});

/** A route going round a 10 m circle 100 times (2000 points): points piled in one place. */
function knot(id: string, phase: number): Feature {
  return through(
    id,
    Array.from({ length: 2000 }, (_, k) => {
      const a = ((k + phase) * 2 * Math.PI) / 20;
      return [LNG0 + (10 * Math.cos(a)) / M_PER_DEG_LNG, LAT0 + (10 * Math.sin(a)) / M_PER_DEG_LAT];
    }),
  );
}

/** One bad call: its routes and options, and the details its error must have besides the code. */
interface Case {
  routes: RoutesInput;
  options: SideBySideOptions;
  details: Record<string, unknown>;
  message: string | RegExp;
}

/** An error's details: only the fields it has. */
function detailsOf(error: unknown): Record<string, unknown> {
  const fields = ['routeId', 'stationId', 'index', 'point', 'option'];
  return Object.fromEntries(
    fields.flatMap((key) =>
      typeof error === 'object' && error !== null && key in error
        ? [[key, Reflect.get(error, key)]]
        : [],
    ),
  );
}

const ZOOM = { zoom: 15 };

/** Every input error code, with an input that gets it. Keyed by code: the compiler checks all
 * are here. */
const INPUT_CASES: Record<InputErrorCode, () => Case> = {
  BAD_OPTION: () => ({
    routes: routes(east('A')),
    options: { zoom: 15, lineWidth: -1 },
    details: { option: 'lineWidth' },
    message: '`lineWidth` must be greater than 0.',
  }),
  BAD_ROUTES: () => ({
    routes: routes(east('A'), { ...east('B'), geometry: { type: 'Point', coordinates: [0, 0] } }),
    options: ZOOM,
    details: { routeId: 'B', index: 1 },
    message: 'Route "B": geometry must be a LineString.',
  }),
  MISSING_ID: () => ({
    routes: routes(east('A'), { ...east('B'), id: undefined }),
    options: ZOOM,
    details: { index: 1 },
    message: /^Feature 1 needs an `id`/,
  }),
  DUPLICATE_ID: () => ({
    routes: routes(east('A'), east('B', 50), east('A', 100)),
    options: ZOOM,
    details: { routeId: 'A', index: 2 },
    message: 'Route "A" appears twice (features 0 and 2).',
  }),
  TOO_MANY_ROUTES: () => ({
    routes: routes(...Array.from({ length: 71 }, (_, i) => east(i, i))),
    options: ZOOM,
    details: {},
    message: 'Too many routes: 71 given, at most 70 allowed.',
  }),
  // Refused by the WASM: a latitude off the map.
  BAD_COORDINATE: () => ({
    routes: routes(
      east('A'),
      through('B', [
        [LNG0, LAT0],
        [LNG0, 95],
      ]),
    ),
    options: ZOOM,
    details: { routeId: 'B', index: 1, point: 1 },
    message:
      'Route "B", point 1: needs a finite longitude from -180 to 180 and latitude from -85.05 to 85.05',
  }),
  TOO_FEW_POINTS: () => ({
    routes: routes(
      east('A'),
      through(7, [
        [LNG0, LAT0],
        [LNG0, LAT0],
      ]),
    ),
    options: ZOOM,
    details: { routeId: 7, index: 1 },
    message: 'Route 7: needs at least two distinct points',
  }),
  TOO_MANY_POINTS: () => ({
    routes: routes(
      east('A'),
      through(
        'B',
        Array.from({ length: 1_000_000 }, (_, i) => [LNG0 + i * 1e-9, LAT0]),
      ),
    ),
    options: ZOOM,
    details: { routeId: 'B', index: 1 },
    message: /^Too many points: route "B" brings the total to 1000002/,
  }),
  CROSSES_ANTIMERIDIAN: () => ({
    routes: routes(
      through('fiji', [
        [179.98, -16.8],
        [179.99, -16.8],
        [-179.99, -16.8],
      ]),
    ),
    options: ZOOM,
    details: { routeId: 'fiji', index: 0, point: 2 },
    message: /^Route "fiji", point 2: more than 180° of longitude from the point before/,
  }),
  TOO_WIDE: () => ({
    routes: routes(
      through('far', [
        [LNG0, LAT0],
        [LNG0 + 3, LAT0],
      ]),
    ),
    options: ZOOM,
    details: {},
    message: /at most 200 km each way$/,
  }),
  TOO_LONG: () => {
    // Laps of 190 km east and west, 1 km apart, inside the 200 km box: over 2000 km together.
    const lap = 190_000 / M_PER_DEG_LNG;
    const laps = Array.from({ length: 12 }, (_, k) => [
      [LNG0 + (k % 2) * lap, LAT0 + (k * 1000) / M_PER_DEG_LAT],
      [LNG0 + ((k + 1) % 2) * lap, LAT0 + (k * 1000) / M_PER_DEG_LAT],
    ]).flat();
    return {
      routes: routes(through('long', laps)),
      options: ZOOM,
      details: {},
      message: /km long together; at most 2000 km$/,
    };
  },
  TOO_COMPLEX: () => ({
    routes: routes(...Array.from({ length: 10 }, (_, i) => knot(`knot ${i}`, i * 0.37))),
    options: ZOOM,
    details: {},
    message: /^too complex: too many points lie close together/,
  }),
  // Refused by the WASM: a station off the map, named by its id.
  BAD_STATION: () => ({
    routes: routes(east('A')),
    options: {
      zoom: 15,
      stations: {
        type: 'FeatureCollection',
        features: [station('ok', [LNG0, LAT0]), station('pole', [LNG0, 95])],
      },
    },
    details: { stationId: 'pole', index: 1 },
    message: /^Station "pole": needs a finite longitude/,
  }),
  TOO_MANY_STATIONS: () => ({
    routes: routes(east('A')),
    options: {
      zoom: 15,
      stations: {
        type: 'FeatureCollection',
        features: Array.from({ length: 5001 }, () => station(undefined, [LNG0, LAT0])),
      },
    },
    details: {},
    message: 'Too many stations: 5001 given, at most 5000 allowed.',
  }),
};

describe('input error codes', () => {
  it.each(Object.entries(INPUT_CASES))(
    '%s: an InputError with that code and its details',
    async (code, make) => {
      const { routes: input, options, details, message } = make();
      const error = await sideBySide(input, options).then(
        () => undefined,
        (e: unknown) => e,
      );
      expect(error).toBeInstanceOf(InputError);
      expect(error).toBeInstanceOf(BraidError);
      expect(error).not.toBeInstanceOf(InternalError);
      expect(error).toMatchObject({ name: 'InputError', code });
      expect(detailsOf(error)).toEqual(details);
      expect(error instanceof Error ? error.message : '').toMatch(message);
      // A refusal leaves it working.
      expect((await sideBySide(routes(east('A')), ZOOM)).lines.features).toHaveLength(1);
    },
    15_000,
  );
});

/** A layout the engine can't read: an odd number of coordinates. */
function badLayout(): unknown {
  try {
    run({
      id: 1,
      type: 'sideBySide',
      coords: new Float64Array([LNG0, LAT0, LNG0]),
      starts: new Uint32Array([0]),
      stations: new Float64Array(),
      zoom: 15,
      metersPerPixel: undefined,
      lineWidth: 4,
      spacing: 1,
    });
  } catch (error) {
    return error;
  }
  return undefined;
}

/** The internal codes and the TERMINATED code, each with how the test gets it. */
const OTHER_CASES: Record<InternalErrorCode | 'TERMINATED', () => Promise<unknown>> = {
  // A crash needs the crash projects' capped memory: recovery.crash*.test.ts gets the real one.
  // Here: the error a crash gives, as run() makes it.
  ENGINE_CRASHED: async () =>
    internalError('ENGINE_CRASHED', 'the engine crashed (RuntimeError: unreachable).'),
  WORKER_FAILED: async () =>
    internalError('WORKER_FAILED', 'the worker failed (its thread stopped) and was stopped.'),
  // The engine refusing a layout the package should never send: internal, not the input's fault.
  INTERNAL: async () => asBraidError(badLayout(), ['A'], []),
  TERMINATED: async () => {
    const worker = createBraidWorker();
    worker.terminate();
    return worker.sideBySide(routes(east('A')), ZOOM).then(
      () => undefined,
      (e: unknown) => e,
    );
  },
};

describe('internal error codes and TERMINATED', () => {
  it('an engine refusal of the layout reaches the caller as INTERNAL, naming the refusal', async () => {
    expect(badLayout()).toBeInstanceOf(Refused);
    const error = await OTHER_CASES.INTERNAL();
    expect(error).toBeInstanceOf(InternalError);
    expect(error).toMatchObject({
      code: 'INTERNAL',
      message:
        'Internal error in transit-braid, not a problem with your input: the engine refused the call (BAD_LAYOUT: coords length must be even (lng, lat pairs), got 3).',
    });
  });

  it.each(Object.entries(OTHER_CASES))('%s: its class and code', async (code, make) => {
    const error = await make();
    expect(error).toBeInstanceOf(BraidError);
    expect(error).toMatchObject({ code });
    expect(error).not.toBeInstanceOf(InputError);
    expect(error instanceof InternalError).toBe(code !== 'TERMINATED');
  });

  it('anything else thrown inside becomes INTERNAL, keeping its message', () => {
    const error = asBraidError(new TypeError('boom'), [], []);
    expect(error).toBeInstanceOf(InternalError);
    expect(error.message).toBe(
      'Internal error in transit-braid, not a problem with your input: boom',
    );
    expect(asBraidError('text', [], []).message).toMatch(/: text$/);
  });
});
