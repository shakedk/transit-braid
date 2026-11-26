// Crash recovery in Node, with the real WASM (`pnpm build:wasm` first). The `crash` project
// (vitest.config.ts) caps each WebAssembly memory at 8 MB, so a big input really runs the WASM
// out of memory: Rust aborts, a trap like any panic. Without recovery the trap left the WASM's
// cache locked, so every later call failed ("busy") and its memory was never freed.
import { afterEach, describe, expect, it } from 'vitest';

import { connect } from './connection.ts';
import { BraidError, InternalError } from './errors.ts';
import { nodePort, startThread } from './node-port.ts';
import {
  type BraidWorker,
  createBraidWorker,
  createSession,
  type RoutesInput,
  sideBySide,
} from './node.ts';

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

/** The error a settled call rejected with; fails the test if it resolved instead. */
function reasonOf(result: PromiseSettledResult<unknown> | undefined): unknown {
  if (result?.status !== 'rejected') throw new Error(`expected a rejection, got ${result?.status}`);
  return result.reason;
}

/** Runs `work` on a new worker thread (its own WASM instance, so a fresh cache), then stops it. */
async function onFreshWorker<T>(work: (worker: BraidWorker) => Promise<T>): Promise<T> {
  const fresh = createBraidWorker();
  try {
    return await within(work(fresh));
  } finally {
    fresh.terminate();
  }
}

/** Crashes the WASM on this thread, then draws `small` with it. */
async function crashThenDraw(): Promise<void> {
  await expect(within(sideBySide(huge, OPTIONS))).rejects.toThrow(CRASHED);
  expect((await within(sideBySide(small, OPTIONS))).lines.features).toHaveLength(3);
}

describe('crash recovery in transit-braid/node, on this thread', () => {
  it('runs with the WASM memory capped, so the huge input really crashes', () => {
    expect(process.execArgv).toContain('--wasm-max-mem-pages=128');
  });

  it('fails only the call that crashed: a fresh instance answers the next one', async () => {
    const fresh = await within(sideBySide(small, OPTIONS));

    const crash = within(sideBySide(huge, OPTIONS));
    await expect(crash).rejects.toBeInstanceOf(InternalError);
    await expect(crash).rejects.toMatchObject({ code: 'ENGINE_CRASHED', message: CRASHED });

    expect(await within(sideBySide(small, OPTIONS))).toEqual(fresh);
    // And it keeps working as before: a zoom change draws what a fresh instance draws.
    const zoomed = await onFreshWorker(async (w) => w.sideBySide(small, { zoom: 17 }));
    expect(await within(sideBySide(small, { zoom: 17 }))).toEqual(zoomed);
  });

  it('crashes again on the same input, and recovers again each time', async () => {
    expect.assertions(6);
    await crashThenDraw();
    await crashThenDraw();
    await crashThenDraw();
  });

  it('keeps sessions working after a crash: each starts over with an empty memory', async () => {
    const fresh = await within(sideBySide(edited, OPTIONS));
    const session = createSession();
    try {
      await within(session.sideBySide(small, OPTIONS));
      // A crash in a pure call takes every session's memory with the crashed instance.
      await expect(within(sideBySide(huge, OPTIONS))).rejects.toThrow(CRASHED);
      // The session's next call (an edit) draws as a first call would, with no stale object.
      expect(await within(session.sideBySide(edited, OPTIONS))).toEqual(fresh);
      // A crash inside the session's own call: the same, and the session goes on.
      await expect(within(session.sideBySide(huge, OPTIONS))).rejects.toMatchObject({
        code: 'ENGINE_CRASHED',
      });
      expect(await within(session.sideBySide(edited, OPTIONS))).toEqual(fresh);
      expect(await within(session.sideBySide(small, { zoom: 17 }))).toEqual(
        await within(sideBySide(small, { zoom: 17 })),
      );
    } finally {
      session.close();
    }
    // Closed after the crash: nothing to free in the new instance, and nothing breaks.
    expect((await within(sideBySide(small, OPTIONS))).lines.features).toHaveLength(3);
  });

  it("drops the plain call's kept input with the crashed instance: its zoom change draws fresh", async () => {
    const zoomed = await onFreshWorker(async (w) => w.sideBySide(small, { zoom: 17 }));
    // The plain call keeps `small`'s graph for a zoom change; the crash is in a session's call,
    // so nothing replaced it before the instance went.
    await within(sideBySide(small, OPTIONS));
    const session = createSession();
    try {
      await expect(within(session.sideBySide(huge, OPTIONS))).rejects.toMatchObject({
        code: 'ENGINE_CRASHED',
      });
    } finally {
      session.close();
    }
    expect(await within(sideBySide(small, { zoom: 17 }))).toEqual(zoomed);
    expect(await within(sideBySide(small, OPTIONS))).toEqual(
      await onFreshWorker(async (w) => w.sideBySide(small, OPTIONS)),
    );
  });
});

describe('crash recovery in a transit-braid/node worker thread', () => {
  let worker: BraidWorker | undefined;
  afterEach(() => worker?.terminate());

  it('fails only the call that crashed; the calls waiting behind it are answered', async () => {
    worker = createBraidWorker();
    const fresh = await within(worker.sideBySide(small, OPTIONS));
    const [crash, after] = await within(
      Promise.allSettled([worker.sideBySide(huge, OPTIONS), worker.sideBySide(small, OPTIONS)]),
    );
    expect(crash).toEqual({
      status: 'rejected',
      reason: new InternalError('ENGINE_CRASHED', CRASHED),
    });
    expect(after).toEqual({ status: 'fulfilled', value: fresh });
  });

  it('rejects the waiting calls of a thread that stops, then every later call at once', async () => {
    const thread = startThread();
    const stopping = connect(nodePort(thread));
    // Stopped from outside before it even loaded: nothing can answer these.
    const waiting = [stopping.sideBySide(small, OPTIONS), stopping.sideBySide(edited, OPTIONS)];
    await thread.terminate();
    const [call, other] = await within(Promise.allSettled(waiting));
    // Exit code 0 or 1, depending on how far it got.
    const failed = {
      code: 'WORKER_FAILED',
      message: expect.stringMatching(
        /^Internal error in transit-braid, not a problem with your input: the worker failed \(its thread stopped with exit code \d\) and was stopped\.$/,
      ),
    };
    expect(reasonOf(call)).toBeInstanceOf(InternalError);
    expect(reasonOf(call)).toMatchObject(failed);
    expect(reasonOf(other)).toMatchObject(failed);
    expect(stopping.alive).toBe(false);

    const later = within(stopping.sideBySide(small, OPTIONS), 100);
    await expect(later).rejects.toBeInstanceOf(InternalError);
    await expect(later).rejects.toMatchObject({ code: 'WORKER_FAILED' });
    await expect(later).rejects.toThrow(
      /the worker failed earlier \(its thread stopped with exit code \d\) and was stopped\. Start a new one with createBraidWorker\(\)\.$/,
    );
  });

  it('rejects every call on a terminated worker at once, and the waiting ones', async () => {
    worker = createBraidWorker();
    await within(worker.sideBySide(small, OPTIONS)); // fully started
    const waiting = worker.sideBySide(edited, OPTIONS);
    worker.terminate();
    await expect(within(waiting)).rejects.toThrow('Braid worker was terminated.');
    await expect(within(worker.sideBySide(small, OPTIONS), 100)).rejects.toThrow(
      new BraidError('TERMINATED', 'Braid worker was terminated.'),
    );
  });
});
