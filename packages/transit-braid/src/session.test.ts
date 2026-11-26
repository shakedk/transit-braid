// A pure sideBySide() and sessions, in transit-braid/node with the real WASM (`pnpm build:wasm`
// first). The committed stress grid (70 routes) is an input whose edit, drawn warm from the call
// before, differs from a fresh draw: so it tells a pure call from one with a memory.
import { readFile } from 'node:fs/promises';
import { performance } from 'node:perf_hooks';

import { afterEach, describe, expect, it } from 'vitest';

import { initSync } from '../wasm/transit_braid_wasm.js';
import { BraidError } from './errors.ts';
import {
  type BraidSession,
  type BraidWorker,
  createBraidWorker,
  createSession,
  type RoutesInput,
  type SideBySideOptions,
  sideBySide,
  type StationsInput,
} from './node.ts';

/** Meters per degree where hand-made fixtures are placed, around 0°, 0°. */
const PLACE_M_PER_DEG = 111_319.490_793_273_57;

interface MeterFixture {
  features: { properties: { line: string }; geometry: { coordinates: number[][] } }[];
}

/** The stress grid, as routes; `moved`: its first route's middle point moved about 12 m. */
async function stressGrid(moved = false): Promise<RoutesInput> {
  const text = await readFile(
    new URL('../../../fixtures/stress/stress-grid.geojson', import.meta.url),
    'utf8',
  );
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- a committed fixture, checked by the fixtures crate
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
        type: 'Feature' as const,
        id: f.properties.line,
        properties: null,
        geometry: { type: 'LineString' as const, coordinates },
      };
    }),
  };
}

/** A small, unrelated network: two routes on one street in Tel Aviv. */
const street: RoutesInput = {
  type: 'FeatureCollection',
  features: ['A', 'B'].map((id) => ({
    type: 'Feature' as const,
    id,
    properties: null,
    geometry: {
      type: 'LineString' as const,
      coordinates: [
        [34.78, 32.07],
        [34.785, 32.07],
      ],
    },
  })),
};

const OPTIONS = { zoom: 15 };

/** The WASM's memory, in bytes: it grows when something keeps memory and never shrinks. */
async function wasmMemory(): Promise<number> {
  const bytes = await readFile(new URL('../wasm/transit_braid_wasm_bg.wasm', import.meta.url));
  // Already loaded by the calls before: this gives the running instance.
  return initSync({ module: bytes }).memory.buffer.byteLength;
}

let sessions: BraidSession[] = [];
let worker: BraidWorker | undefined;
afterEach(() => {
  for (const s of sessions) s.close();
  sessions = [];
  worker?.terminate();
  worker = undefined;
});

/** A session closed after the test. */
function open(): BraidSession {
  const session = createSession();
  sessions.push(session);
  return session;
}

/** What a fresh call draws: a new session's first call starts from an empty cache. */
async function fresh(routes: RoutesInput, options: SideBySideOptions): Promise<unknown> {
  const session = createSession();
  try {
    return await session.sideBySide(routes, options);
  } finally {
    session.close();
  }
}

/** How long a plain call of `routes` at `zoom` takes, in ms. */
async function time(routes: RoutesInput, zoom: number): Promise<number> {
  const start = performance.now();
  await sideBySide(routes, { zoom });
  return performance.now() - start;
}

/** The stress grid with a bus stop at the middle of its first 20 routes' first segments. */
async function gridWithStops(): Promise<[RoutesInput, StationsInput]> {
  const grid = await stressGrid();
  const stops: StationsInput = {
    type: 'FeatureCollection',
    features: grid.features.slice(0, 20).map((route, i) => {
      const [[x0 = 0, y0 = 0] = [], [x1 = 0, y1 = 0] = []] = route.geometry.coordinates;
      return {
        type: 'Feature' as const,
        id: `stop-${i}`,
        properties: { name: `Stop ${i % 7}` },
        geometry: { type: 'Point' as const, coordinates: [(x0 + x1) / 2, (y0 + y1) / 2] },
      };
    }),
  };
  return [grid, stops];
}

describe('sideBySide() is pure', () => {
  it('draws the same input the same way whatever ran before it', async () => {
    const [grid, moved] = await Promise.all([stressGrid(), stressGrid(true)]);
    const first = await sideBySide(moved, OPTIONS);
    // Calls that used to leave their graph in the shared cache: the original grid (so the next
    // call was an edit of it), another network, another zoom, and a session's calls.
    await sideBySide(grid, OPTIONS);
    await sideBySide(street, { zoom: 17 });
    const session = open();
    await session.sideBySide(grid, OPTIONS);
    await sideBySide(grid, OPTIONS);
    expect(await sideBySide(moved, OPTIONS)).toEqual(first);
  });

  it('gives each of many calls at once the answer it gives alone', async () => {
    const [grid, moved] = await Promise.all([stressGrid(), stressGrid(true)]);
    const alone = await sideBySide(moved, OPTIONS);
    const [, together] = await Promise.all([
      sideBySide(grid, OPTIONS),
      sideBySide(moved, OPTIONS),
      sideBySide(street, OPTIONS),
    ]);
    expect(together).toEqual(alone);
  });
});

describe("sideBySide()'s fast zoom: the last input's graph kept, nothing else", () => {
  it('redraws a zoom, scale, width or spacing change with the bits a fresh call draws', async () => {
    const [grid, stops] = await gridWithStops();
    await sideBySide(street, OPTIONS); // another input first: the grid's first call is fresh
    expect(await sideBySide(grid, { zoom: 15, stations: stops })).toEqual(
      await fresh(grid, { zoom: 15, stations: stops }),
    );
    for (const options of [
      { zoom: 17, stations: stops },
      { zoom: 12.5, stations: stops },
      { metersPerPixel: 0.8, stations: stops },
      { zoom: 16, lineWidth: 6, spacing: 2, stations: stops },
      { zoom: 15, stations: stops },
    ]) {
      // oxlint-disable-next-line eslint/no-await-in-loop -- in turn, on purpose: each reuses the last
      expect(await sideBySide(grid, options)).toEqual(await fresh(grid, options));
    }
  });

  it('is much faster for a zoom change than for a first draw', async () => {
    const grid = await stressGrid();
    const moved = await stressGrid(true);
    const first: number[] = [];
    const zooms: number[] = [];
    for (let i = 0; i < 3; i++) {
      // oxlint-disable-next-line eslint/no-await-in-loop -- timed one at a time, on purpose
      first.push(await time(i % 2 === 0 ? grid : moved, 15));
      for (const zoom of [16, 17]) {
        // oxlint-disable-next-line eslint/no-await-in-loop -- timed one at a time, on purpose
        zooms.push(await time(i % 2 === 0 ? grid : moved, zoom));
      }
    }
    // About 140 ms against under 1 ms here: a margin no busy machine closes.
    expect(Math.min(...zooms)).toBeLessThan(Math.min(...first) / 4);
  });

  it('draws changed routes, stations or a changed cap from scratch: what a fresh call draws', async () => {
    const [grid, stops] = await gridWithStops();
    const moved = await stressGrid(true);
    const reversed = { ...grid, features: grid.features.toReversed() };
    const fewerStops = { ...stops, features: stops.features.slice(1) };
    await sideBySide(grid, { zoom: 15, stations: stops });
    for (const [routes, options] of [
      [moved, { zoom: 15, stations: stops }],
      [moved, { zoom: 15, stations: fewerStops }],
      [moved, { zoom: 15 }],
      [moved, { zoom: 15, maxSideBySide: 2 }],
      [moved, { zoom: 17, maxSideBySide: 2 }],
      [moved, { zoom: 17, maxSideBySide: 3 }],
      [reversed, { zoom: 17, maxSideBySide: 3 }],
      [grid, { zoom: 17 }],
    ] as const) {
      // oxlint-disable-next-line eslint/no-await-in-loop -- in turn, on purpose: each follows the last
      expect(await sideBySide(routes, options)).toEqual(await fresh(routes, options));
    }
  });

  it('never starts an edit from the call before: a session would differ', async () => {
    const [grid, moved] = await Promise.all([stressGrid(), stressGrid(true)]);
    await sideBySide(grid, OPTIONS);
    const plain = await sideBySide(moved, OPTIONS);
    expect(plain).toEqual(await fresh(moved, OPTIONS));
    const session = open();
    await session.sideBySide(grid, OPTIONS);
    expect(await session.sideBySide(moved, OPTIONS)).not.toEqual(plain);
    // And a zoom change after the edit: the fresh draw's zoom, not the session's.
    expect(await sideBySide(moved, { zoom: 17 })).toEqual(await fresh(moved, { zoom: 17 }));
  });

  it('stays pure: other calls in between change nothing', async () => {
    const [grid, moved] = await Promise.all([stressGrid(), stressGrid(true)]);
    const first = await sideBySide(grid, OPTIONS);
    const session = open();
    await session.sideBySide(grid, OPTIONS);
    await session.sideBySide(moved, { zoom: 17 });
    await sideBySide(grid, { zoom: 17, maxSideBySide: 2 });
    await sideBySide(street, OPTIONS);
    worker = createBraidWorker();
    await worker.sideBySide(moved, OPTIONS);
    await sideBySide(grid, { zoom: 12 });
    expect(await sideBySide(grid, OPTIONS)).toEqual(first);
    expect(first).toEqual(await fresh(grid, OPTIONS));
  });

  it('keeps the last good input through refused calls and calls with no routes', async () => {
    const grid = await stressGrid();
    await sideBySide(grid, OPTIONS);
    // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- deliberately wrong input
    const nothing = null as unknown as RoutesInput;
    await expect(sideBySide(nothing, OPTIONS)).rejects.toMatchObject({ code: 'BAD_ROUTES' });
    await expect(sideBySide(grid, { zoom: 99 })).rejects.toMatchObject({ code: 'BAD_OPTION' });
    await expect(sideBySide(grid, { zoom: 15, maxSideBySide: 0 })).rejects.toMatchObject({
      code: 'BAD_OPTION',
    });
    const none = await sideBySide({ type: 'FeatureCollection', features: [] }, OPTIONS);
    expect(none.lines.features).toEqual([]);
    expect(await sideBySide(grid, { zoom: 17 })).toEqual(await fresh(grid, { zoom: 17 }));
    // Refused by the engine (routes 300 km apart): the next call draws from scratch.
    const far: RoutesInput = {
      type: 'FeatureCollection',
      features: [
        street.features[0]!,
        {
          ...street.features[1]!,
          geometry: {
            type: 'LineString',
            coordinates: [
              [37.5, 32.07],
              [37.51, 32.07],
            ],
          },
        },
      ],
    };
    await expect(sideBySide(far, OPTIONS)).rejects.toMatchObject({ code: 'TOO_WIDE' });
    expect(await sideBySide(grid, { zoom: 16 })).toEqual(await fresh(grid, { zoom: 16 }));
  });

  it('frees the input it replaces: changing input again and again leaves the WASM no bigger', async () => {
    const [grid, moved] = await Promise.all([stressGrid(), stressGrid(true)]);
    await sideBySide(grid, OPTIONS);
    await sideBySide(moved, OPTIONS);
    await sideBySide(grid, OPTIONS);
    const before = await wasmMemory();
    for (let i = 0; i < 6; i++) {
      // oxlint-disable-next-line eslint/no-await-in-loop -- one call at a time, on purpose
      await sideBySide(i % 2 === 0 ? moved : grid, { zoom: 15 + (i % 3) });
    }
    expect(await wasmMemory()).toBe(before);
  });

  it('does the same on a worker of your own (worker_threads)', async () => {
    const [grid, moved] = await Promise.all([stressGrid(), stressGrid(true)]);
    worker = createBraidWorker();
    await worker.sideBySide(grid, OPTIONS);
    expect(await worker.sideBySide(grid, { zoom: 17 })).toEqual(await fresh(grid, { zoom: 17 }));
    expect(await worker.sideBySide(moved, { zoom: 17 })).toEqual(await fresh(moved, { zoom: 17 }));
  });
});

describe('a session', () => {
  it('remembers its last call: an edit is drawn warm, unlike a pure call', async () => {
    const [grid, moved] = await Promise.all([stressGrid(), stressGrid(true)]);
    const session = open();
    expect(await session.sideBySide(grid, OPTIONS)).toEqual(await sideBySide(grid, OPTIONS));
    const warm = await session.sideBySide(moved, OPTIONS);
    // The warm order keeps the lanes the routes had; a fresh solve may pick another.
    expect(warm).not.toEqual(await sideBySide(moved, OPTIONS));
    // The same steps in a new session give the same answers: a session is deterministic.
    const again = open();
    await again.sideBySide(grid, OPTIONS);
    expect(await again.sideBySide(moved, OPTIONS)).toEqual(warm);
  });

  it('keeps its own memory: another session and pure calls in between change nothing', async () => {
    const [grid, moved] = await Promise.all([stressGrid(), stressGrid(true)]);
    const alone = open();
    await alone.sideBySide(grid, OPTIONS);
    const expected = await alone.sideBySide(moved, OPTIONS);

    const mine = open();
    const other = open();
    await mine.sideBySide(grid, OPTIONS);
    await other.sideBySide(street, OPTIONS);
    await other.sideBySide(moved, { zoom: 12 });
    await sideBySide(street, OPTIONS);
    expect(await mine.sideBySide(moved, OPTIONS)).toEqual(expected);
  });

  it('runs its calls in the order made, even when they are made at once', async () => {
    const [grid, moved] = await Promise.all([stressGrid(), stressGrid(true)]);
    const inTurn = open();
    await inTurn.sideBySide(grid, OPTIONS);
    const expected = await inTurn.sideBySide(moved, OPTIONS);
    const atOnce = open();
    const [, edit] = await Promise.all([
      atOnce.sideBySide(grid, OPTIONS),
      atOnce.sideBySide(moved, OPTIONS),
    ]);
    expect(edit).toEqual(expected);
  });

  it('refuses bad input as sideBySide() does, and keeps its memory', async () => {
    const [grid, moved] = await Promise.all([stressGrid(), stressGrid(true)]);
    const reference = open();
    await reference.sideBySide(grid, OPTIONS);
    const expected = await reference.sideBySide(moved, OPTIONS);

    const session = open();
    await session.sideBySide(grid, OPTIONS);
    // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- deliberately wrong input
    const nothing = null as unknown as RoutesInput;
    await expect(session.sideBySide(nothing, OPTIONS)).rejects.toMatchObject({
      code: 'BAD_ROUTES',
    });
    await expect(session.sideBySide(grid, { zoom: 99 })).rejects.toMatchObject({
      code: 'BAD_OPTION',
      option: 'zoom',
    });
    const offMap = {
      ...street,
      features: [
        {
          ...street.features[0]!,
          geometry: {
            type: 'LineString' as const,
            coordinates: [
              [0, 0],
              [0, 89],
            ],
          },
        },
      ],
    };
    await expect(session.sideBySide(offMap, OPTIONS)).rejects.toMatchObject({
      code: 'BAD_COORDINATE',
    });
    expect(await session.sideBySide(moved, OPTIONS)).toEqual(expected);
  });

  it('rejects calls after close() with code CLOSED; a call made before still finishes', async () => {
    const [grid, moved] = await Promise.all([stressGrid(), stressGrid(true)]);
    const reference = open();
    await reference.sideBySide(grid, OPTIONS);
    const expected = await reference.sideBySide(moved, OPTIONS);

    const session = createSession();
    await session.sideBySide(grid, OPTIONS);
    const before = session.sideBySide(moved, OPTIONS);
    session.close();
    expect(await before).toEqual(expected);
    const after = session.sideBySide(grid, OPTIONS);
    await expect(after).rejects.toBeInstanceOf(BraidError);
    await expect(after).rejects.toMatchObject({
      name: 'BraidError',
      code: 'CLOSED',
      message: 'Braid session was closed.',
    });
    session.close(); // again: nothing
    await expect(session.sideBySide(street, OPTIONS)).rejects.toMatchObject({ code: 'CLOSED' });
    // Bad input on a closed session: still CLOSED, checked first.
    // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- deliberately wrong input
    const nothing = null as unknown as RoutesInput;
    await expect(session.sideBySide(nothing, OPTIONS)).rejects.toMatchObject({ code: 'CLOSED' });
  });

  it('frees its memory on close(): sessions closed after use leave the WASM no bigger', async () => {
    const grid = await stressGrid();
    // Warm up: the memory a pure call of the grid needs, and one session of it.
    await sideBySide(grid, OPTIONS);
    const first = createSession();
    await first.sideBySide(grid, OPTIONS);
    first.close();
    await sideBySide(street, OPTIONS); // the close is handled before this call
    const before = await wasmMemory();
    for (let i = 0; i < 6; i++) {
      const session = createSession();
      // oxlint-disable-next-line eslint/no-await-in-loop -- one session at a time, on purpose
      await session.sideBySide(grid, OPTIONS);
      session.close();
    }
    await sideBySide(street, OPTIONS);
    expect(await wasmMemory()).toBe(before);
    // Left open, each keeps its memory (about 10 MB for this grid), past the free space earlier
    // tests left behind: the check can tell.
    for (let i = 0; i < 8; i++) {
      // oxlint-disable-next-line eslint/no-await-in-loop -- one session at a time, on purpose
      await open().sideBySide(grid, OPTIONS);
    }
    expect(await wasmMemory()).toBeGreaterThan(before);
  });

  it('gives no routes no lines, without touching its memory', async () => {
    const [grid, moved] = await Promise.all([stressGrid(), stressGrid(true)]);
    const reference = open();
    await reference.sideBySide(grid, OPTIONS);
    const expected = await reference.sideBySide(moved, OPTIONS);
    const session = open();
    await session.sideBySide(grid, OPTIONS);
    const none = await session.sideBySide({ type: 'FeatureCollection', features: [] }, OPTIONS);
    expect(none.lines.features).toEqual([]);
    expect(await session.sideBySide(moved, OPTIONS)).toEqual(expected);
  });
});

describe('a session on a worker of your own (worker_threads)', () => {
  it("gives the same answers as one on this thread, and pure calls the worker's own", async () => {
    const [grid, moved] = await Promise.all([stressGrid(), stressGrid(true)]);
    const here = open();
    await here.sideBySide(grid, OPTIONS);
    const expected = await here.sideBySide(moved, OPTIONS);

    worker = createBraidWorker();
    const there = worker.createSession();
    await there.sideBySide(grid, OPTIONS);
    expect(await there.sideBySide(moved, OPTIONS)).toEqual(expected);
    // The worker's pure call: as fresh as this thread's, after the session's calls.
    expect(await worker.sideBySide(moved, OPTIONS)).toEqual(await sideBySide(moved, OPTIONS));
    there.close();
    await expect(there.sideBySide(grid, OPTIONS)).rejects.toMatchObject({ code: 'CLOSED' });
  });

  it('rejects with TERMINATED once its worker is terminated', async () => {
    worker = createBraidWorker();
    const session = worker.createSession();
    await session.sideBySide(street, OPTIONS);
    worker.terminate();
    await expect(session.sideBySide(street, OPTIONS)).rejects.toMatchObject({
      code: 'TERMINATED',
    });
    session.close(); // on a stopped worker: nothing, no error
  });
});
