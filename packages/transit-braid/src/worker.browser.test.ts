// Runs in a real browser (Chromium) via Vitest browser mode.
// Proves a worker of your own starts, finds and loads the .wasm through `new URL`, and draws.
import { afterEach, describe, expect, it } from 'vitest';

import { type BraidWorker, createBraidWorker } from './client.ts';
import { InputError } from './errors.ts';
import type { RoutesInput } from './routes.ts';
import { createSession, sideBySide } from './side-by-side.ts';

let worker: BraidWorker | undefined;
afterEach(() => worker?.terminate());

/** `n` routes on one 500 m street in Tel Aviv, ids `0`…`n − 1`. */
function street(n: number): RoutesInput {
  return {
    type: 'FeatureCollection',
    features: Array.from({ length: n }, (_, id) => ({
      type: 'Feature' as const,
      id,
      properties: { id },
      geometry: {
        type: 'LineString' as const,
        coordinates: [
          [34.78, 32.07],
          [34.785, 32.07],
        ],
      },
    })),
  };
}

describe('Braid worker in a real browser', () => {
  it('runs sideBySide() on this worker, with the same answer and errors as the shared one', async () => {
    worker = createBraidWorker();
    const routes = street(2);
    expect(await worker.sideBySide(routes, { zoom: 16 })).toEqual(
      await sideBySide(routes, { zoom: 16 }),
    );
    // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- deliberately wrong input
    const bad = worker.sideBySide(null as unknown as RoutesInput, { zoom: 16 });
    await expect(bad).rejects.toBeInstanceOf(InputError);
    await expect(bad).rejects.toMatchObject({ code: 'BAD_ROUTES' });
    await expect(worker.sideBySide(routes, {})).rejects.toMatchObject({
      code: 'BAD_OPTION',
      option: 'zoom',
    });
  });

  it('keeps the caller’s input usable (copied, not moved)', async () => {
    worker = createBraidWorker();
    const routes = street(2);
    const before = JSON.stringify(routes);
    await worker.sideBySide(routes, { zoom: 16 });
    expect(JSON.stringify(routes)).toBe(before);
  });

  it('matches answers to requests when many run at once', async () => {
    worker = createBraidWorker();
    const w = worker;
    const counts = [1, 2, 3, 4, 5, 6, 7, 8];
    const results = await Promise.all(counts.map((n) => w.sideBySide(street(n), { zoom: 15 })));
    expect(results.map((r) => r.lines.features.length)).toEqual(counts);
  });

  it('keeps a session of its own: calls in turn, CLOSED after close(), TERMINATED after terminate()', async () => {
    worker = createBraidWorker();
    const session = worker.createSession();
    const shared = createSession();
    const twins = (order: string[]): RoutesInput => ({
      type: 'FeatureCollection',
      features: order.map((id) => ({ ...street(1).features[0]!, id })),
    });
    // Two copies of one street listed the other way round: each keeps its line, in both sessions.
    for (const order of [
      ['a', 'b'],
      ['b', 'a'],
    ]) {
      // oxlint-disable-next-line eslint/no-await-in-loop -- in turn on purpose: one memory
      const [mine, theirs] = await Promise.all([
        session.sideBySide(twins(order), { metersPerPixel: 1 }),
        shared.sideBySide(twins(order), { metersPerPixel: 1 }),
      ]);
      expect(mine).toEqual(theirs);
    }
    shared.close();
    session.close();
    await expect(session.sideBySide(street(1), { zoom: 16 })).rejects.toMatchObject({
      code: 'CLOSED',
    });
    const other = worker.createSession();
    worker.terminate();
    await expect(other.sideBySide(street(1), { zoom: 16 })).rejects.toMatchObject({
      code: 'TERMINATED',
    });
  });

  it('rejects pending calls when terminated', async () => {
    worker = createBraidWorker();
    const call = worker.sideBySide(street(2), { zoom: 16 });
    worker.terminate();
    await expect(call).rejects.toMatchObject({ code: 'TERMINATED' });
  });

  it('rejects every call at once after it is terminated (it used to never answer)', async () => {
    worker = createBraidWorker();
    await worker.sideBySide(street(1), { zoom: 16 }); // fully started
    worker.terminate();
    const late = worker.sideBySide(street(1), { zoom: 16 });
    const timeout = new Promise((resolve) => setTimeout(() => resolve('no answer'), 300));
    await expect(Promise.race([late, timeout])).rejects.toMatchObject({
      code: 'TERMINATED',
      message: 'Braid worker was terminated.',
    });
  });
});
