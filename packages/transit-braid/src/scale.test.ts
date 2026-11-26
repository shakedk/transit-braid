// The map scale: exactly one of `zoom` (512-pixel tiles) or `metersPerPixel`, through
// transit-braid/node and the real WASM (`pnpm build:wasm` first). A zoom and the meters per pixel
// it stands for draw the same lines; a 256-pixel-tile map's zoom − 1 is the same scale.
import { describe, expect, it } from 'vitest';

import { InputError } from './errors.ts';
import { type RoutesInput, sideBySide, type SideBySideOptions } from './node.ts';

/** Web Mercator's sphere radius in meters (EPSG:3857). */
const EARTH_RADIUS_M = 6_378_137;

/**
 * Ground meters per pixel at `latitude` on a Web Mercator map at `zoom` with `tile`-pixel tiles,
 * as the docs give it: 2π × R × cos(latitude) / (tile × 2^zoom).
 */
function metersPerPixel(zoom: number, latitude: number, tile: number): number {
  return (2 * Math.PI * EARTH_RADIUS_M * Math.cos((latitude * Math.PI) / 180)) / (tile * 2 ** zoom);
}

/** A route `id` through `points` (`[lng, lat]`). */
function route(id: string, points: number[][]): RoutesInput['features'][number] {
  return {
    type: 'Feature',
    id,
    properties: null,
    geometry: { type: 'LineString', coordinates: points },
  };
}

/** Routes sharing a street, crossing it and branching off, around `[lng0, lat0]` (degrees). */
function network(lng0: number, lat0: number): RoutesInput {
  const at = (east: number, north: number): number[] => [
    lng0 + east / (111_320 * Math.cos((lat0 * Math.PI) / 180)),
    lat0 + north / 110_900,
  ];
  return {
    type: 'FeatureCollection',
    features: [
      route('A', [at(-600, 0), at(600, 0)]),
      route('B', [at(-600, 0), at(0, 0), at(400, 300)]),
      route('C', [at(600, 0), at(-600, 0)]),
      route('D', [at(0, -500), at(0, 500)]),
    ],
  };
}

/** The middle latitude of the routes' bounding box: where the scale is measured. */
function middleLatitude(routes: RoutesInput): number {
  const lats = routes.features.flatMap((f) => f.geometry.coordinates.map((p) => p[1] ?? 0));
  return (Math.min(...lats) + Math.max(...lats)) / 2;
}

/** The largest distance between the same point of two results, in degrees. */
async function farthestApart(
  routes: RoutesInput,
  a: SideBySideOptions,
  b: SideBySideOptions,
): Promise<number> {
  const [one, two] = await Promise.all([sideBySide(routes, a), sideBySide(routes, b)]);
  let worst = 0;
  for (const [i, line] of one.lines.features.entries()) {
    const other = two.lines.features[i]?.geometry.coordinates ?? [];
    expect(other).toHaveLength(line.geometry.coordinates.length);
    for (const [k, [x = 0, y = 0]] of line.geometry.coordinates.entries()) {
      const [u = Number.NaN, v = Number.NaN] = other[k] ?? [];
      worst = Math.max(worst, Math.abs(x - u), Math.abs(y - v));
    }
  }
  return worst;
}

describe('the map scale', () => {
  it('draws the same lines, to the bit, for a zoom and the meters per pixel it stands for', async () => {
    // Around the equator (the routes' middle latitude is exactly 0), so cos(latitude) is 1 and
    // the formula gives the engine's own numbers.
    const routes = network(0, 0);
    expect(middleLatitude(routes)).toBe(0);
    for (const zoom of [11, 14, 17]) {
      // oxlint-disable-next-line eslint/no-await-in-loop -- one zoom at a time, each compared
      const byZoom = await sideBySide(routes, { zoom });
      // oxlint-disable-next-line eslint/no-await-in-loop -- one zoom at a time, each compared
      const byMeters = await sideBySide(routes, { metersPerPixel: metersPerPixel(zoom, 0, 512) });
      expect(byMeters).toEqual(byZoom);
    }
  });

  it('matches away from the equator too, within float rounding (Tel Aviv, fractional zooms)', async () => {
    const routes = network(34.78, 32.07);
    const latitude = middleLatitude(routes);
    for (const zoom of [12.5, 15.37, 18]) {
      const zoomed = { zoom };
      const metered = { metersPerPixel: metersPerPixel(zoom, latitude, 512) };
      // oxlint-disable-next-line eslint/no-await-in-loop -- one zoom at a time, each compared
      expect(await farthestApart(routes, zoomed, metered)).toBeLessThan(1e-12);
    }
    // While a zoom one level off is a whole pitch off: the check can tell scales apart.
    const off = { metersPerPixel: metersPerPixel(15, latitude, 512) };
    expect(await farthestApart(routes, { zoom: 16 }, off)).toBeGreaterThan(1e-6);
  });

  it("takes a 256-pixel-tile map (Leaflet's) as its zoom − 1, or by its meters per pixel", async () => {
    const routes = network(0, 0);
    for (const leafletZoom of [12, 15, 18]) {
      // oxlint-disable-next-line eslint/no-await-in-loop -- one zoom at a time, each compared
      const asZoom = await sideBySide(routes, { zoom: leafletZoom - 1 });
      // oxlint-disable-next-line eslint/no-await-in-loop -- one zoom at a time, each compared
      const asMeters = await sideBySide(routes, {
        metersPerPixel: metersPerPixel(leafletZoom, 0, 256),
      });
      expect(asMeters).toEqual(asZoom);
    }
  });

  it.each([
    [{}, 'zoom', 'neither was given'],
    [{ lineWidth: 4 }, 'zoom', 'neither was given'],
    [{ zoom: undefined, metersPerPixel: undefined }, 'zoom', 'neither was given'],
    [{ zoom: 15, metersPerPixel: 1 }, 'metersPerPixel', 'both were given'],
    [{ zoom: 0, metersPerPixel: 1e9 }, 'metersPerPixel', 'both were given'],
  ])('refuses %j: exactly one of them is a BAD_OPTION error', async (options, option, why) => {
    // Plain JavaScript may pass `undefined` for both, whatever the types say.
    // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- deliberately loose input
    const refused = sideBySide(network(0, 0), options as SideBySideOptions);
    await expect(refused).rejects.toBeInstanceOf(InputError);
    await expect(refused).rejects.toMatchObject({
      code: 'BAD_OPTION',
      option,
      message: `Give exactly one of \`zoom\` or \`metersPerPixel\`: ${why}.`,
    });
  });

  it.each([
    [{ zoom: -1 }, 'zoom', '`zoom` must be between 0 and 24.'],
    [{ zoom: 24.5 }, 'zoom', '`zoom` must be between 0 and 24.'],
    [{ zoom: Number.NaN }, 'zoom', '`zoom` must be a finite number.'],
    [{ metersPerPixel: 0 }, 'metersPerPixel', '`metersPerPixel` must be greater than 0.'],
    [{ metersPerPixel: -2 }, 'metersPerPixel', '`metersPerPixel` must be greater than 0.'],
    [
      { metersPerPixel: Number.POSITIVE_INFINITY },
      'metersPerPixel',
      '`metersPerPixel` must be a finite number.',
    ],
  ])('refuses a scale out of range: %j', async (options, option, message) => {
    await expect(sideBySide(network(0, 0), options)).rejects.toMatchObject({
      code: 'BAD_OPTION',
      option,
      message,
    });
  });

  it('draws at both ends of the zoom range', async () => {
    const routes = network(0, 0);
    for (const zoom of [0, 24]) {
      // oxlint-disable-next-line eslint/no-await-in-loop -- one zoom at a time
      const out = await sideBySide(routes, { zoom });
      expect(out.lines.features).toHaveLength(4);
      expect(
        out.lines.features.every((f) => f.geometry.coordinates.flat().every(Number.isFinite)),
      ).toBe(true);
    }
  });
});
