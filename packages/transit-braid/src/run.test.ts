// run(): the request dispatcher shared by the worker and the Node entry, on the real WASM in
// Node (`pnpm build:wasm` first).
import { readFile } from 'node:fs/promises';

import { beforeAll, describe, expect, it } from 'vitest';

import { initSync } from '../wasm/transit_braid_wasm.js';
import { InternalError, Refused } from './errors.ts';
import type { SideBySideCall } from './protocol.ts';
import { closeSession, run } from './run.ts';

beforeAll(async () => {
  initSync({
    module: await readFile(new URL('../wasm/transit_braid_wasm_bg.wasm', import.meta.url)),
  });
});

const sideBySide = (coords: number[], starts: number[], cap?: number): SideBySideCall => ({
  id: 1,
  type: 'sideBySide',
  coords: new Float64Array(coords),
  starts: new Uint32Array(starts),
  stations: new Float64Array(),
  zoom: 16,
  metersPerPixel: undefined,
  lineWidth: 4,
  spacing: 1,
  maxSideBySide: cap,
});

describe('run', () => {
  it('runs the pipeline: one line per route, with where each starts, and no bars without stations', () => {
    const street = [34.78, 32.07, 34.785, 32.07];
    const out = run(sideBySide([...street, ...street], [0, 2]));
    expect([...out.starts]).toEqual([0, 2]);
    expect(out.coords).toHaveLength(8);
    expect([...out.bars]).toEqual([]);
    expect([...out.dropped]).toEqual([]);
  });

  it('runs a capped call: the routes left out, and the kept ones only', () => {
    const street = [34.78, 32.07, 34.785, 32.07];
    const out = run(sideBySide([...street, ...street, ...street], [0, 2, 4], 2));
    expect([...out.dropped]).toEqual([2]);
    expect([...out.starts]).toEqual([0, 2]);
  });

  it("keeps a session's cache by its id, starts afresh without one, and frees it on close", () => {
    // Two routes on one street (the same points), listed one way, then the other way round with
    // their keys: a session matches them by key, so each keeps its line; a call without a
    // session draws them by place.
    const street = [34.78, 32.07, 34.785, 32.07];
    const call = (keys: number[], session?: number): number[][] => {
      const out = run({
        ...sideBySide([...street, ...street], [0, 2]),
        routeKeys: new Uint32Array(keys),
        session,
      });
      return [Array.from(out.coords.subarray(0, 4)), Array.from(out.coords.subarray(4))];
    };
    const [first, second] = call([1, 2], 41);
    expect(first).not.toEqual(second);
    expect(call([2, 1], 41)).toEqual([second, first]);
    expect(call([2, 1])).toEqual([first, second]);
    // Closed: the next call with that id gets a new, empty cache, so it draws by place.
    closeSession(41);
    expect(call([2, 1], 41)).toEqual([first, second]);
    closeSession(41);
    closeSession(12_345); // never opened: nothing to do
  });

  it('passes the engine refusal on as data, its Rust side freed', () => {
    let refused: unknown;
    try {
      run(sideBySide([34.78, 95, 34.79, 95], [0]));
    } catch (error) {
      refused = error;
    }
    expect(refused).toBeInstanceOf(Refused);
    expect(refused instanceof Refused && refused.refusal).toEqual({
      code: 'BAD_COORDINATE',
      reason: 'needs a finite longitude from -180 to 180 and latitude from -85.05 to 85.05',
      route: 0,
      point: 0,
      station: undefined,
    });
    // The engine still works: a refusal is not a crash.
    expect([...run(sideBySide([34.78, 32.07, 34.785, 32.07], [0])).starts]).toEqual([0]);
  });

  it('refuses an unknown request type with an internal error, never a hang', () => {
    // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- deliberately wrong input
    const unknown = { id: 3, type: 'nope' } as unknown as SideBySideCall;
    expect(() => run(unknown)).toThrow(
      new InternalError(
        'INTERNAL',
        'Internal error in transit-braid, not a problem with your input: unknown request type nope.',
      ),
    );
    expect(() => run(unknown)).toThrow(InternalError);
  });
});
