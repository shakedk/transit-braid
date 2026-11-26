// Crash recovery in a real Chromium, through the shared Web Worker of `sideBySide()`. The
// `crash-browser` project (vitest.config.ts) caps each WebAssembly memory at 8 MB, so a big input
// really runs the WASM out of memory: Rust aborts, a trap like any panic.
import { afterEach, describe, expect, it } from 'vitest';

import { type BraidWorker, createBraidWorker, startWorker } from './client.ts';
import { connect, type PipelineWorker, sharedWorker } from './connection.ts';
import { InternalError } from './errors.ts';
import type { RoutesInput } from './routes.ts';
import { sideBySideOn } from './side-by-side-on.ts';
import { sideBySide } from './side-by-side.ts';
import { webPort } from './web-port.ts';

const LAT0 = 32.07;
const LNG0 = 34.78;
const M_PER_DEG_LAT = 110_900;
const M_PER_DEG_LNG = 94_300;

/** A route `id` with `points` points along a 2 km street, `northM` meters north, zigzagging 1 m. */
function street(id: string, points = 2, northM = 0): RoutesInput['features'][number] {
  return {
    type: 'Feature',
    id,
    properties: { id },
    geometry: {
      type: 'LineString',
      coordinates: Array.from({ length: points }, (_, k) => [
        LNG0 + (k * 2000) / (points - 1) / M_PER_DEG_LNG,
        LAT0 + (northM + (k % 2)) / M_PER_DEG_LAT,
      ]),
    },
  };
}

const routes = (...features: RoutesInput['features']): RoutesInput => ({
  type: 'FeatureCollection',
  features,
});

/** Three routes on one street: about 1.3 MB of WASM memory. */
const small = routes(street('A'), street('B'), street('C', 2, 30));
/** The same, with route C moved 10 m: an edit of `small`. */
const edited = routes(street('A'), street('B'), street('C', 2, 40));
/** 50 routes of 2000 points on one street: about 17 MB of WASM memory, over the 8 MB cap. */
const huge = routes(...Array.from({ length: 50 }, (_, i) => street(`r${i}`, 2000, 2 * i)));

const OPTIONS = { zoom: 16 };
const CRASHED =
  'Internal error in transit-braid, not a problem with your input: the engine crashed (RuntimeError: unreachable). It restarted, so the next call starts fresh.';

/** `promise`, or a rejection if it hasn't settled within `ms`: no test waits forever. */
async function within<T>(promise: Promise<T>, ms = 5000): Promise<T> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  const late = new Promise<never>((_, reject) => {
    timer = setTimeout(() => reject(new Error(`no answer within ${ms} ms`)), ms);
  });
  try {
    return await Promise.race([promise, late]);
  } finally {
    clearTimeout(timer);
  }
}

/** A real worker whose script throws as it starts: it never answers, and reports an error. */
function failingWorker(): PipelineWorker {
  const script = new Blob(['throw new Error("broken worker");'], { type: 'text/javascript' });
  return connect(webPort(new Worker(URL.createObjectURL(script), { type: 'module' })));
}

let worker: BraidWorker | undefined;
afterEach(() => worker?.terminate());

describe('crash recovery in the browser', () => {
  it('fails only the call that crashed: the shared worker answers the next one fresh', async () => {
    worker = createBraidWorker();
    const fresh = await within(worker.sideBySide(small, OPTIONS));
    const freshEdit = await within(worker.sideBySide(edited, OPTIONS));

    await within(sideBySide(small, OPTIONS));
    const crash = within(sideBySide(huge, OPTIONS));
    await expect(crash).rejects.toBeInstanceOf(InternalError);
    await expect(crash).rejects.toMatchObject({ code: 'ENGINE_CRASHED', message: CRASHED });
    // The edit draws as a fresh call would: the warm start from before the crash is gone.
    expect(await within(sideBySide(edited, OPTIONS))).toEqual(freshEdit);
    expect(await within(sideBySide(small, OPTIONS))).toEqual(fresh);
  });

  it('answers the calls waiting behind a crash', async () => {
    worker = createBraidWorker();
    const fresh = await within(worker.sideBySide(small, OPTIONS));
    const [crash, after] = await within(
      Promise.allSettled([sideBySide(huge, OPTIONS), sideBySide(small, OPTIONS)]),
    );
    expect(crash).toEqual({
      status: 'rejected',
      reason: new InternalError('ENGINE_CRASHED', CRASHED),
    });
    expect(after).toEqual({ status: 'fulfilled', value: fresh });
  });

  it('stops a worker that fails: its waiting calls reject, later ones at once', async () => {
    const failing = failingWorker();
    const waiting = within(failing.sideBySide(small, OPTIONS));
    await expect(waiting).rejects.toMatchObject({
      name: 'InternalError',
      code: 'WORKER_FAILED',
      message:
        'Internal error in transit-braid, not a problem with your input: the worker failed (Uncaught Error: broken worker) and was stopped.',
    });
    expect(failing.alive).toBe(false);
    const later = within(failing.sideBySide(small, OPTIONS), 100);
    await expect(later).rejects.toBeInstanceOf(InternalError);
    await expect(later).rejects.toMatchObject({ code: 'WORKER_FAILED' });
    await expect(later).rejects.toThrow(
      'the worker failed earlier (Uncaught Error: broken worker) and was stopped. Start a new one with createBraidWorker().',
    );
  });

  it('starts a new shared worker after the shared one failed, and only then', async () => {
    const started: PipelineWorker[] = [];
    const shared = sharedWorker(() => {
      const next = started.length === 0 ? failingWorker() : startWorker();
      started.push(next);
      return next;
    });
    const failed = within(sideBySideOn(shared, small, OPTIONS));
    await expect(failed).rejects.toBeInstanceOf(InternalError);
    await expect(failed).rejects.toMatchObject({ code: 'WORKER_FAILED' });
    await expect(failed).rejects.toThrow('the worker failed (Uncaught Error: broken worker)');

    const expected = await within(sideBySide(small, OPTIONS));
    expect(await within(sideBySideOn(shared, small, OPTIONS))).toEqual(expected);
    expect((await within(sideBySideOn(shared, edited, OPTIONS))).lines.features).toHaveLength(3);
    expect(started).toHaveLength(2);
    started[1]?.terminate();
  });
});
