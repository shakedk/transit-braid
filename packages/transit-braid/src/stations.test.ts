import { describe, expect, it } from 'vitest';

import { InputError, InternalError } from './errors.ts';
import { buildBars, flattenStations } from './stations.ts';

const stop = (id: unknown, coordinates: unknown, properties: unknown = { name: 'Dizengoff' }) => ({
  type: 'Feature',
  id,
  properties,
  geometry: { type: 'Point', coordinates },
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

/** The details a bad stations option gets, from its message: the station's place when one is at
 * fault (always the first), and its id when it has one. */
function expectedDetails(message: string): Record<string, unknown> {
  const stationId = message.startsWith('Station "S"')
    ? 'S'
    : message.startsWith('Station 7')
      ? 7
      : null;
  return {
    code: 'BAD_STATION',
    ...(message.startsWith('Station') ? { index: 0 } : {}),
    ...(stationId === null ? {} : { stationId }),
  };
}

describe('flattenStations', () => {
  it('flattens stations, ids optional, altitudes ignored', () => {
    const flat = flattenStations(
      collection(stop('S1', [34.78, 32.07]), stop(undefined, [34.79, 32.08, 12], null)),
    );
    expect(flat.ids).toEqual(['S1', undefined]);
    expect(flat.properties).toEqual([{ name: 'Dizengoff' }, null]);
    expect([...flat.coords]).toEqual([34.78, 32.07, 34.79, 32.08]);
  });

  it('numbers stop names so same-name stops can share a bar', () => {
    const flat = flattenStations(
      collection(
        stop('a', [0, 0], { name: 'Dizengoff' }),
        stop('b', [0, 0], { name: 'Allenby' }),
        stop('c', [0, 0], { name: 'Dizengoff' }),
        stop('d', [0, 0], { name: '' }),
        stop('e', [0, 0], { name: 7 }),
        stop('f', [0, 0], null),
        stop('g', [0, 0], {}),
        stop('h', [0, 0], { name: 'dizengoff' }),
      ),
    );
    // From 1 in order of first appearance; no usable name is 0; names compare exactly.
    expect([...flat.groups]).toEqual([1, 2, 1, 0, 0, 0, 0, 3]);
    expect(flattenStations(undefined).groups.length).toBe(0);
  });

  it('takes no stations as none', () => {
    for (const none of [undefined, null]) expect(flattenStations(none).ids).toEqual([]);
    expect(flattenStations(collection()).coords.length).toBe(0);
  });

  it('refuses more than 5,000 stations, but takes 5,000', () => {
    const many = Array.from({ length: 5001 }, (_, i) => stop(i, [0, 0]));
    expect(() => flattenStations(collection(...many))).toThrow(
      new InputError('TOO_MANY_STATIONS', 'Too many stations: 5001 given, at most 5000 allowed.'),
    );
    expect(flattenStations(collection(...many.slice(0, 5000))).ids).toHaveLength(5000);
  });

  it.each([
    [42, '`stations` must be a GeoJSON FeatureCollection of Points.'],
    ['stops', '`stations` must be a GeoJSON FeatureCollection of Points.'],
    [[], '`stations` must be a GeoJSON FeatureCollection of Points.'],
    [{ type: 'FeatureCollection' }, '`stations.features` must be an array.'],
    [collection(null), 'Station at index 0 is not a GeoJSON Feature.'],
    [collection(stop('', [0, 0])), 'Station at index 0: `id` must be a non-empty string'],
    [collection(stop(Number.NaN, [0, 0])), 'Station at index 0: `id` must be a non-empty string'],
    [collection(stop({}, [0, 0])), 'Station at index 0: `id` must be a non-empty string'],
    [collection(stop('S', [0, 0], 'x')), 'Station "S": properties must be an object or null.'],
    [
      collection({ ...stop('S', [0, 0]), geometry: { type: 'LineString', coordinates: [] } }),
      'Station "S": geometry must be a Point.',
    ],
    [
      collection({ ...stop('S', [0, 0]), geometry: null }),
      'Station "S": geometry must be a Point.',
    ],
    [collection(stop(7, [0])), 'Station 7: needs [longitude, latitude] numbers.'],
    [collection(stop(7, ['0', 0])), 'Station 7: needs [longitude, latitude] numbers.'],
    [
      collection(stop(undefined, [0, Number.NaN])),
      'Station at index 0: needs [longitude, latitude]',
    ],
    [collection(stop(undefined, null)), 'Station at index 0: needs [longitude, latitude]'],
  ])('refuses a bad stations option, naming the station: %j', (stations, message) => {
    let error: unknown;
    try {
      flattenStations(stations);
    } catch (caught) {
      error = caught;
    }
    expect(error).toBeInstanceOf(InputError);
    expect(error).toMatchObject({ message: expect.stringContaining(message) });
    expect(detailsOf(error)).toEqual(expectedDetails(message));
  });
});

describe('buildBars', () => {
  const flat = { ids: ['S1', undefined, 9], properties: [{ name: 'A' }, null, { name: 'C' }] };

  it('makes one LineString per station with a bar, keeping ids and copying properties', () => {
    const bars = new Float64Array([1, 2, 3, 4, NaN, NaN, NaN, NaN, 5, 6, 7, 8]);
    const out = buildBars(flat, bars);
    expect(out).toEqual({
      type: 'FeatureCollection',
      features: [
        {
          type: 'Feature',
          id: 'S1',
          properties: { name: 'A' },
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
          id: 9,
          properties: { name: 'C' },
          geometry: {
            type: 'LineString',
            coordinates: [
              [5, 6],
              [7, 8],
            ],
          },
        },
      ],
    });
    expect(out.features[0]?.properties).not.toBe(flat.properties[0]);
  });

  it('leaves out the id of a station that has none', () => {
    const out = buildBars({ ids: [undefined], properties: [null] }, new Float64Array([1, 2, 3, 4]));
    expect(out.features[0]).not.toHaveProperty('id');
  });

  it('refuses an answer with the wrong number of bars', () => {
    expect(() => buildBars(flat, new Float64Array(8))).toThrow(
      new InternalError(
        'INTERNAL',
        'Internal error in transit-braid, not a problem with your input: 2 bars came back for 3 stations.',
      ),
    );
    expect(buildBars({ ids: [], properties: [] }, new Float64Array()).features).toEqual([]);
  });
});
