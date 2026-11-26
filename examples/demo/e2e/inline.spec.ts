// transit-braid/inline in a real Chromium, as a bundler that can't follow
// `new URL(..., import.meta.url)` would ship it: the page gets only the one built file
// (`dist/inline.js`, `pnpm build:packages`), every other request fails, and `new URL(path, base)`
// throws on the page.
import { readFile } from 'node:fs/promises';
import { join } from 'node:path';

import { expect, type Page, test } from '@playwright/test';
import type * as Inline from 'transit-braid/inline';

const ORIGIN = 'http://inline.braid.test';
const BUILT = join(
  import.meta.dirname,
  '..',
  '..',
  '..',
  'packages',
  'transit-braid',
  'dist',
  'inline.js',
);

/** The page: no `new URL(path, base)`, then the inline entry as `window.braid`. */
const PAGE = `<!doctype html>
<meta charset="utf-8" />
<title>transit-braid/inline</title>
<script>
  const NativeURL = URL;
  window.URL = class extends NativeURL {
    constructor(url, base) {
      if (base !== undefined) throw new Error('new URL(path, base) is not supported here');
      super(url);
    }
  };
</script>
<script type="module">
  import * as braid from './transit-braid.js';
  window.braid = braid;
</script>`;

declare global {
  interface Window {
    braid?: typeof Inline;
  }
}

/** One straight route `id` at latitude `lat`. */
const oneRoute = (id: string, lat: number): Inline.RoutesInput => ({
  type: 'FeatureCollection',
  features: [
    {
      type: 'Feature',
      id,
      properties: null,
      geometry: {
        type: 'LineString',
        coordinates: [
          [34.78, lat],
          [34.79, lat],
        ],
      },
    },
  ],
});

/** Opens the page; returns every URL it asked for and the page's uncaught errors. */
async function open(page: Page): Promise<{ asked: string[]; errors: string[] }> {
  const script = await readFile(BUILT, 'utf8').catch(() => {
    throw new Error(`${BUILT} is missing: run pnpm build:packages`);
  });
  const asked: string[] = [];
  const errors: string[] = [];
  page.on('pageerror', (e) => errors.push(e.message));
  await page.route('**/*', async (route) => {
    const url = route.request().url();
    asked.push(url);
    if (url === `${ORIGIN}/`) {
      await route.fulfill({ contentType: 'text/html', body: PAGE });
    } else if (url === `${ORIGIN}/transit-braid.js` || url === `${ORIGIN}/transit-braid.js?copy`) {
      await route.fulfill({ contentType: 'text/javascript', body: script });
    } else {
      await route.abort();
    }
  });
  await page.goto(`${ORIGIN}/`);
  await expect.poll(() => page.evaluate(() => typeof window.braid?.sideBySide)).toBe('function');
  return { asked, errors };
}

test('the inline entry draws routes side by side from its one file, without new URL', async ({
  page,
}) => {
  const { asked, errors } = await open(page);
  // The page really has no `new URL(path, base)`.
  const blocked = await page.evaluate(() => {
    try {
      return String(new URL('a.wasm', 'http://inline.braid.test/'));
    } catch (e) {
      return e instanceof Error ? e.message : String(e);
    }
  });
  expect(blocked).toBe('new URL(path, base) is not supported here');

  const out = await page.evaluate(async () => {
    const braid = window.braid;
    if (!braid) throw new Error('no braid');
    const street = [
      [34.78, 32.07],
      [34.785, 32.07],
    ];
    const result = await braid.sideBySide(
      {
        type: 'FeatureCollection',
        features: ['A', 'B'].map((id) => ({
          type: 'Feature' as const,
          id,
          properties: { color: id === 'A' ? '#ff0000' : null },
          geometry: { type: 'LineString' as const, coordinates: street },
        })),
      },
      { zoom: 16 },
    );
    const empty = await braid.sideBySide({ type: 'FeatureCollection', features: [] }, { zoom: 16 });
    return {
      ids: result.lines.features.map((f) => f.id),
      colors: result.lines.features.map((f) => f.properties?.['color'] ?? null),
      lats: result.lines.features.map((f) => f.geometry.coordinates[0]?.[1] ?? Number.NaN),
      empty: empty.lines.features.length,
    };
  });
  expect(out.ids).toEqual(['A', 'B']);
  expect(out.colors).toEqual(['#ff0000', null]);
  // One street, two lanes: the lines moved apart, one to each side of it.
  const [a = Number.NaN, b = Number.NaN] = out.lats;
  expect(a - 32.07).toBeCloseTo(-(b - 32.07), 9);
  expect(Math.abs(a - b)).toBeGreaterThan(0);
  expect(out.empty).toBe(0);

  // Nothing but the page and the one file was asked for: no worker file, no .wasm.
  expect(asked).toEqual([`${ORIGIN}/`, `${ORIGIN}/transit-braid.js`]);
  expect(errors).toEqual([]);
});

/** The names every entry exports (public.ts plus the two calls), sorted. */
const PUBLIC_NAMES = [
  'BraidError',
  'DEFAULT_LINE_WIDTH',
  'DEFAULT_SPACING',
  'InputError',
  'InternalError',
  'createBraidWorker',
  'createSession',
  'sideBySide',
];

test('the inline entry refuses bad input with a named error, and its own worker answers', async ({
  page,
}) => {
  const { errors } = await open(page);
  const refused = await page.evaluate(
    async ([far, ok]) => {
      const braid = window.braid;
      if (!braid) throw new Error('no braid');
      /** Calls `sideBySide()` with deliberately wrong input; gives the error's name and message. */
      const refusal = async (routes: unknown, options: unknown): Promise<string> =>
        braid
          // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- deliberately wrong input
          .sideBySide(routes as Inline.RoutesInput, options as Inline.SideBySideOptions)
          .then(
            () => 'resolved',
            (e: unknown) =>
              e instanceof braid.InputError ? `${e.name} ${e.code}: ${e.message}` : String(e),
          );
      const reasons = await Promise.all([
        // A latitude off the map: refused by the WASM inside the inline worker.
        refusal(far, { zoom: 16 }),
        // Refused before the worker: a missing zoom, and null instead of routes.
        refusal(ok, {}),
        refusal(null, { zoom: 16 }),
      ]);
      // A worker of your own from the inline file works too.
      const worker = braid.createBraidWorker();
      const own = await worker.sideBySide(ok, { zoom: 16 });
      const shared = await braid.sideBySide(ok, { zoom: 16 });
      // And sessions, on its own worker and on the shared one.
      const sessions = [worker.createSession(), braid.createSession()];
      const drawn = await Promise.all(sessions.map(async (s) => s.sideBySide(ok, { zoom: 16 })));
      for (const s of sessions) s.close();
      const closed = await sessions[1]!.sideBySide(ok, { zoom: 16 }).then(
        () => 'drawn',
        (e: unknown) => (e instanceof braid.BraidError ? e.code : String(e)),
      );
      worker.terminate();
      return {
        sessions: drawn.map((d) => JSON.stringify(d) === JSON.stringify(shared)),
        closed,
        reasons,
        names: Object.keys(braid).toSorted(),
        same: JSON.stringify(own) === JSON.stringify(shared),
      };
    },
    [oneRoute('far', 95), oneRoute('ok', 32.07)] as const,
  );
  const [far, noZoom, noRoutes] = refused.reasons;
  expect(far).toMatch(/^InputError BAD_COORDINATE: Route "far", point 0: /);
  expect(noZoom).toMatch(/^InputError BAD_OPTION: /);
  expect(noRoutes).toMatch(/^InputError BAD_ROUTES: /);
  // The same public names as every other entry.
  expect(refused.names).toEqual(PUBLIC_NAMES);
  // Its own sideBySide() and both sessions give the shared one's answer.
  expect(refused.same).toBe(true);
  expect(refused.sessions).toEqual([true, true]);
  expect(refused.closed).toBe('CLOSED');
  expect(errors).toEqual([]);
});

test('instanceof works across two copies of the package (the inline file loaded twice)', async ({
  page,
}) => {
  const { errors } = await open(page);
  const checks = await page.evaluate(
    async (route) => {
      const braid = window.braid;
      if (!braid) throw new Error('no braid');
      // The same file at another URL is a second module: its own classes, its own worker.
      // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- the same built file
      const copy = (await import(`${location.origin}/transit-braid.js?copy`)) as typeof braid;
      // oxlint-disable-next-line unicorn/consistent-function-scoping -- runs in the page: only this function is sent
      const caught = async (call: Promise<unknown>): Promise<unknown> =>
        call.then(
          () => undefined,
          (e: unknown) => e,
        );
      const options = await caught(copy.sideBySide(route, {}));
      // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- deliberately wrong input
      const noRoutes = null as unknown as Inline.RoutesInput;
      const input = await caught(copy.sideBySide(noRoutes, { zoom: 16 }));
      return {
        twoCopies: copy.InputError !== braid.InputError,
        optionsIsInput: options instanceof braid.InputError,
        optionsIsBraid: options instanceof braid.BraidError,
        optionsIsInternal: options instanceof braid.InternalError,
        inputIsInput: input instanceof braid.InputError,
        code: input instanceof braid.InputError ? input.code : null,
        plainIsInput: new Error('x') instanceof braid.InputError,
        nullIsBraid: (null as unknown) instanceof braid.BraidError,
      };
    },
    oneRoute('ok', 32.07),
  );
  expect(checks).toEqual({
    twoCopies: true,
    optionsIsInput: true,
    optionsIsBraid: true,
    optionsIsInternal: false,
    inputIsInput: true,
    code: 'BAD_ROUTES',
    plainIsInput: false,
    nullIsBraid: false,
  });
  expect(errors).toEqual([]);
});
