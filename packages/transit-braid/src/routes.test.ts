import { describe, expect, it } from 'vitest';

import { InputError, InternalError } from './errors.ts';
import { buildResult, flattenRoutes, type RouteId, RouteKeys } from './routes.ts';

/** A LineString feature as a caller would write it. */
const route = (id: unknown, coordinates: unknown, properties: unknown = {}) => ({
  type: 'Feature',
  id,
  properties,
  geometry: { type: 'LineString', coordinates },
});
const collection = (...features: unknown[]) => ({ type: 'FeatureCollection', features });

/** An error's code and details: only the fields it has. */
function detailsOf(error: unknown): Record<string, unknown> {
  const fields = ['code', 'routeId', 'stationId', 'index', 'point', 'option'];
  return Object.fromEntries(
    fields.flatMap((key) =>
      typeof error === 'object' && error !== null && key in error
        ? [[key, Reflect.get(error, key)]]
        : [],
    ),
  );
}

/** The code and details a bad second route (index 1) gets, from its message. */
function expectedDetails(message: string): Record<string, unknown> {
  const pointAt = message.includes(', point 1:');
  const code = message.includes('needs an `id`')
    ? 'MISSING_ID'
    : pointAt
      ? 'BAD_COORDINATE'
      : 'BAD_ROUTES';
  const routeId = message.startsWith('Route "B"') ? 'B' : message.startsWith('Route 9') ? 9 : null;
  return {
    code,
    index: 1,
    ...(routeId === null ? {} : { routeId }),
    ...(pointAt ? { point: 1 } : {}),
  };
}

/** An internal error as the package makes it. */
function internal(what: string): InternalError {
  return new InternalError(
    'INTERNAL',
    `Internal error in transit-braid, not a problem with your input: ${what}`,
  );
}

/** What `call` throws (fails the test if it returns). */
function thrown(call: () => unknown): unknown {
  try {
    call();
  } catch (error) {
    return error;
  }
  throw new Error('expected a throw');
}
const street = [
  [34.78, 32.07],
  [34.79, 32.07],
];

describe('flattenRoutes', () => {
  it('flattens routes into coords and starts, in order', () => {
    const flat = flattenRoutes(
      collection(
        route('A', street, { color: '#e4002b' }),
        route(
          7,
          [
            [1, 2],
            [3, 4],
            [5, 6],
          ],
          null,
        ),
      ),
    );
    expect(flat.ids).toEqual(['A', 7]);
    expect(flat.properties).toEqual([{ color: '#e4002b' }, null]);
    expect([...flat.coords]).toEqual([34.78, 32.07, 34.79, 32.07, 1, 2, 3, 4, 5, 6]);
    expect([...flat.starts]).toEqual([0, 2]);
  });

  it('accepts no routes, missing properties and ignores altitudes', () => {
    expect(flattenRoutes(collection()).ids).toEqual([]);
    const noProperties = {
      type: 'Feature',
      id: 'A',
      geometry: {
        type: 'LineString',
        coordinates: [
          [1, 2, 30],
          [3, 4, 40],
        ],
      },
    };
    const flat = flattenRoutes(collection(noProperties));
    expect(flat.properties).toEqual([null]);
    expect([...flat.coords]).toEqual([1, 2, 3, 4]);
  });

  it.each([
    [null, 'must be a GeoJSON FeatureCollection'],
    [undefined, 'must be a GeoJSON FeatureCollection'],
    [42, 'must be a GeoJSON FeatureCollection'],
    ['{"type":"FeatureCollection"}', 'must be a GeoJSON FeatureCollection'],
    [[], 'must be a GeoJSON FeatureCollection'],
    [{ type: 'Feature' }, 'must be a GeoJSON FeatureCollection'],
    [{ type: 'FeatureCollection' }, '`routes.features` must be an array'],
    [{ type: 'FeatureCollection', features: null }, '`routes.features` must be an array'],
    [{ type: 'FeatureCollection', features: {} }, '`routes.features` must be an array'],
  ])('refuses a bad collection: %j', (routes, message) => {
    const error = thrown(() => flattenRoutes(routes));
    expect(error).toBeInstanceOf(InputError);
    expect(error).toMatchObject({ code: 'BAD_ROUTES', message: expect.stringContaining(message) });
    expect(error).not.toHaveProperty('index');
  });

  it('refuses more than 70 routes, but takes 70', () => {
    const routes = Array.from({ length: 71 }, (_, i) => route(i, street));
    expect(thrown(() => flattenRoutes(collection(...routes)))).toMatchObject({
      code: 'TOO_MANY_ROUTES',
      message: 'Too many routes: 71 given, at most 70 allowed.',
    });
    expect(flattenRoutes(collection(...routes.slice(0, 70))).ids).toHaveLength(70);
  });

  it('refuses more than a million points, all routes together, but takes a million', () => {
    const half = Array.from({ length: 500_000 }, (_, i) => [34.78 + i * 1e-8, 32.07]);
    const flat = flattenRoutes(collection(route('A', half), route('B', half)));
    expect(flat.coords).toHaveLength(2_000_000);
    const over = collection(route('A', half), route('B', [...half, [34.8, 32.07]]));
    const error = thrown(() => flattenRoutes(over));
    expect(error).toBeInstanceOf(InputError);
    expect(error).toMatchObject({
      code: 'TOO_MANY_POINTS',
      routeId: 'B',
      index: 1,
      message:
        'Too many points: route "B" brings the total to 1000001, at most 1000000 allowed (all routes together).',
    });
  });

  it.each([
    [null, 'Feature 1 is not a GeoJSON Feature'],
    ['A', 'Feature 1 is not a GeoJSON Feature'],
    [{ ...route('B', street), type: 'feature' }, 'Feature 1 is not a GeoJSON Feature'],
    [route(undefined, street), 'Feature 1 needs an `id`'],
    [route(null, street), 'Feature 1 needs an `id`'],
    [route('', street), 'Feature 1 needs an `id`'],
    [route(Number.NaN, street), 'Feature 1 needs an `id`'],
    [route(Infinity, street), 'Feature 1 needs an `id`'],
    [route({ id: 1 }, street), 'Feature 1 needs an `id`'],
    [route(true, street), 'Feature 1 needs an `id`'],
    [route('B', street, 'red'), 'Route "B": properties must be an object or null'],
    [route('B', street, [1]), 'Route "B": properties must be an object or null'],
    [{ ...route('B', street), geometry: null }, 'Route "B": geometry must be a LineString'],
    [
      { ...route('B', street), geometry: { type: 'Point', coordinates: [1, 2] } },
      'Route "B": geometry must be a LineString',
    ],
    [route('B', null), 'Route "B": coordinates must be an array'],
    [route('B', '1,2'), 'Route "B": coordinates must be an array'],
    [route('B', [[1, 2], null]), 'Route "B", point 1: needs [longitude, latitude] numbers'],
    [route('B', [[1, 2], [3]]), 'Route "B", point 1: needs [longitude, latitude] numbers'],
    [
      route('B', [
        [1, 2],
        ['3', 4],
      ]),
      'Route "B", point 1: needs [longitude, latitude] numbers',
    ],
    [
      route('B', [
        [1, 2],
        [3, Number.NaN],
      ]),
      'Route "B", point 1: needs [longitude, latitude]',
    ],
    [
      route('B', [
        [1, 2],
        [3, 4, Infinity],
      ]),
      'Route "B", point 1: needs [longitude, latitude]',
    ],
    [route(9, [[1, 2], {}]), 'Route 9, point 1: needs [longitude, latitude] numbers'],
  ])('refuses a bad route, naming it: %j', (bad, message) => {
    const routes = collection(route('A', street), bad);
    const error = thrown(() => flattenRoutes(routes));
    expect(error).toBeInstanceOf(InputError);
    expect(error).toMatchObject({ message: expect.stringContaining(message) });
    expect(detailsOf(error)).toEqual(expectedDetails(message));
  });

  it('refuses the same id twice, but tells 1 and "1" apart', () => {
    const twice = thrown(() =>
      flattenRoutes(collection(route('A', street), route('B', street), route('A', street))),
    );
    expect(twice).toBeInstanceOf(InputError);
    expect(twice).toMatchObject({
      code: 'DUPLICATE_ID',
      routeId: 'A',
      index: 2,
      message: 'Route "A" appears twice (features 0 and 2).',
    });
    expect(flattenRoutes(collection(route(1, street), route('1', street))).ids).toEqual([1, '1']);
  });

  it('leaves value ranges and short routes to the core', () => {
    // These pass here; the core refuses them, naming the route by its index.
    const flat = flattenRoutes(collection(route('A', [[200, 95]]), route('B', [])));
    expect([...flat.starts]).toEqual([0, 1]);
  });
});

describe('buildResult', () => {
  it('cuts the flat answer into one line per route, with ids and copied properties', () => {
    const properties = { color: '#e4002b' };
    const flat = { ids: ['A', 7] as RouteId[], properties: [properties, null] };
    const out = buildResult(
      flat,
      new Float64Array([1, 2, 3, 4, 5, 6, 7, 8, 9, 10]),
      new Uint32Array([0, 2]),
    );
    expect(out).toEqual({
      lines: {
        type: 'FeatureCollection',
        features: [
          {
            type: 'Feature',
            id: 'A',
            properties,
            geometry: {
              type: 'LineString',
              coordinates: [
                [1, 2],
                [3, 4],
              ],
            },
          },
          {
            type: 'Feature',
            id: 7,
            properties: null,
            geometry: {
              type: 'LineString',
              coordinates: [
                [5, 6],
                [7, 8],
                [9, 10],
              ],
            },
          },
        ],
      },
      stations: { type: 'FeatureCollection', features: [] },
      dropped: [],
    });
    expect(out.lines.features[0]?.properties).not.toBe(properties); // a copy, not the same object
  });

  it('gives empty collections for no routes', () => {
    const out = buildResult({ ids: [], properties: [] }, new Float64Array(), new Uint32Array());
    expect(out.lines.features).toEqual([]);
    expect(out.stations.features).toEqual([]);
    expect(out.dropped).toEqual([]);
  });

  it('leaves out the routes a lane cap dropped and lists their ids', () => {
    const flat = { ids: ['A', 7, 'C', 9] as RouteId[], properties: [null, null, null, null] };
    const out = buildResult(
      flat,
      new Float64Array([1, 2, 3, 4, 5, 6, 7, 8]),
      new Uint32Array([0, 2]),
      new Uint32Array([1, 3]),
    );
    expect(out.lines.features.map((f) => f.id)).toEqual(['A', 'C']);
    expect(out.lines.features.map((f) => f.geometry.coordinates)).toEqual([
      [
        [1, 2],
        [3, 4],
      ],
      [
        [5, 6],
        [7, 8],
      ],
    ]);
    expect(out.dropped).toEqual([7, 9]);
  });

  it('refuses an answer whose kept and dropped routes do not add up', () => {
    const flat = { ids: ['A', 'B'] as RouteId[], properties: [null, null] };
    const lines = new Float64Array([1, 2, 3, 4]);
    expect(() => buildResult(flat, lines, new Uint32Array([0]), new Uint32Array([1, 1]))).toThrow(
      internal('1 lines came back for 2 routes, 2 dropped.'),
    );
    expect(() => buildResult(flat, lines, new Uint32Array([0]), new Uint32Array([5]))).toThrow(
      internal('1 lines came back for 2 routes, 1 dropped.'),
    );
    expect(() => buildResult(flat, lines, new Uint32Array([0, 1]), new Uint32Array([0]))).toThrow(
      internal('2 lines came back for 2 routes, 1 dropped.'),
    );
  });

  it('refuses an answer with the wrong number of lines', () => {
    const flat = { ids: ['A'] as RouteId[], properties: [null] };
    const error = thrown(() =>
      buildResult(flat, new Float64Array([1, 2, 3, 4]), new Uint32Array([0, 1])),
    );
    expect(error).toBeInstanceOf(InternalError);
    expect(error).toMatchObject({
      code: 'INTERNAL',
      message: expect.stringMatching(/2 lines came back for 1 routes\.$/),
    });
  });

  it('round-trips any routes through flattenRoutes (seeded random, 200 cases)', () => {
    let seed = 12_345;
    const random = () => {
      seed = (seed * 1_103_515_245 + 12_345) % 2_147_483_648; // a fixed LCG: same cases every run
      return seed / 2_147_483_648;
    };
    for (let c = 0; c < 200; c++) {
      const routes = Array.from({ length: Math.floor(random() * 6) }, (_, i) =>
        route(
          random() < 0.5 ? i : `r${i}`,
          Array.from({ length: 2 + Math.floor(random() * 5) }, () => [
            random() * 360 - 180,
            random() * 170 - 85,
          ]),
          random() < 0.3 ? null : { n: i },
        ),
      );
      const flat = flattenRoutes(collection(...routes));
      const out = buildResult(flat, flat.coords, flat.starts);
      expect(out.lines.features.map((f) => f.id)).toEqual(routes.map((r) => r.id));
      expect(out.lines.features.map((f) => f.geometry.coordinates)).toEqual(
        routes.map((r) => r.geometry.coordinates),
      );
      expect(out.lines.features.map((f) => f.properties)).toEqual(routes.map((r) => r.properties));
    }
  });
});

describe('RouteKeys', () => {
  it('gives the same id the same key on every call, and different ids different keys', () => {
    const keys = new RouteKeys();
    const first = keys.keysFor(['A', 'B', 7]);
    const second = keys.keysFor([7, 'C', 'A']);
    expect(first).toBeInstanceOf(Uint32Array);
    expect(new Set(first).size).toBe(3);
    expect(second[0]).toBe(first[2]);
    expect(second[2]).toBe(first[0]);
    expect([...first]).not.toContain(second[1]);
  });

  it('numbers each session on its own: another numbering never changes this one', () => {
    const mine = new RouteKeys();
    const before = mine.keysFor(['A', 'B']);
    const other = new RouteKeys();
    other.keysFor(Array.from({ length: 1000 }, (_, i) => `x${i}`));
    expect(mine.keysFor(['A', 'B'])).toEqual(before);
    expect(new RouteKeys().keysFor(['B', 'A'])).toEqual(new Uint32Array([1, 2]));
  });

  it('starts over past 100,000 ids, still one key per id in the call', () => {
    const keys = new RouteKeys();
    keys.keysFor(Array.from({ length: 99_999 }, (_, i) => i));
    const restarted = keys.keysFor(['A', 'B']);
    expect([...restarted]).toEqual([1, 2]);
    expect([...keys.keysFor(['B', 'A'])]).toEqual([2, 1]);
  });

  it('keeps 1 and "1" apart, as the routes do', () => {
    const [n, s] = new RouteKeys().keysFor([1, '1']);
    expect(n).not.toBe(s);
  });

  it('gives no keys for no routes', () => {
    expect(new RouteKeys().keysFor([])).toEqual(new Uint32Array());
  });
});
