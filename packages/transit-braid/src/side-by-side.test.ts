// sideBySide() in Node: everything it checks before starting the worker. The worker path runs
// in a real browser (side-by-side.browser.test.ts).
import { describe, expect, it } from 'vitest';

import { BraidError, InputError } from './errors.ts';
import type { SideBySideOptions } from './options.ts';
import type { RoutesInput } from './routes.ts';
import { sideBySide } from './side-by-side.ts';

const none: RoutesInput = { type: 'FeatureCollection', features: [] };

// JavaScript callers (and JSON) can pass anything, whatever the types say.
// oxlint-disable-next-line typescript/no-unsafe-type-assertion -- deliberately wrong input
const badOptions = (options: unknown): SideBySideOptions => options as SideBySideOptions;
// oxlint-disable-next-line typescript/no-unsafe-type-assertion -- deliberately wrong input
const badRoutes = (routes: unknown): RoutesInput => routes as RoutesInput;

describe('sideBySide before the worker', () => {
  it('checks the stations before the worker too, even with no routes', async () => {
    const bad = { type: 'FeatureCollection', features: [{ type: 'Feature', geometry: null }] };
    await expect(sideBySide(none, badOptions({ zoom: 14, stations: bad }))).rejects.toMatchObject({
      code: 'BAD_STATION',
      index: 0,
      message: 'Station at index 0: geometry must be a Point.',
    });
    await expect(sideBySide(none, badOptions({ zoom: 14, stations: 'x' }))).rejects.toThrow(
      InputError,
    );
    const ok = await sideBySide(none, {
      zoom: 14,
      stations: { type: 'FeatureCollection', features: [] },
    });
    expect(ok.stations).toEqual({ type: 'FeatureCollection', features: [] });
  });

  it('gives empty collections for no routes, without a worker', async () => {
    await expect(sideBySide(none, { zoom: 14 })).resolves.toEqual({
      lines: { type: 'FeatureCollection', features: [] },
      stations: { type: 'FeatureCollection', features: [] },
      dropped: [],
    });
  });

  it.each([
    [{}, 'exactly one of', 'zoom'],
    [{ zoom: 14, metersPerPixel: 1 }, 'exactly one of', 'metersPerPixel'],
    [{ zoom: 25 }, 'between 0 and 24', 'zoom'],
    [{ zoom: 14, lineWidth: 0 }, 'lineWidth', 'lineWidth'],
    [null, '`options` must be an object', 'options'],
    [undefined, '`options` must be an object', 'options'],
    ['zoom 14', '`options` must be an object', 'options'],
  ])(
    'rejects bad options with an InputError, code BAD_OPTION, naming the option: %j',
    async (options, message, option) => {
      const call = sideBySide(none, badOptions(options));
      await expect(call).rejects.toThrow(InputError);
      await expect(sideBySide(none, badOptions(options))).rejects.toThrow(BraidError);
      await expect(sideBySide(none, badOptions(options))).rejects.toMatchObject({
        name: 'InputError',
        code: 'BAD_OPTION',
        option,
        message: expect.stringContaining(message),
      });
    },
  );

  it.each([
    [null, 'BAD_ROUTES', 'must be a GeoJSON FeatureCollection'],
    [
      { type: 'FeatureCollection', features: [{ type: 'Feature' }] },
      'MISSING_ID',
      'Feature 0 needs an `id`',
    ],
  ])(
    'rejects bad routes with an InputError (a promise, never a throw): %j',
    async (routes, code, message) => {
      const call = sideBySide(badRoutes(routes), { zoom: 14 });
      await expect(call).rejects.toThrow(InputError);
      await expect(sideBySide(badRoutes(routes), { zoom: 14 })).rejects.toMatchObject({
        code,
        message: expect.stringContaining(message),
      });
    },
  );
});
