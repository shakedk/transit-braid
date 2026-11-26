// Loads the real WebAssembly build in Node and calls Rust from TypeScript.
// Needs `pnpm build:wasm` first (pnpm check does this).
import { readFile } from 'node:fs/promises';

import { beforeAll, describe, expect, it } from 'vitest';

import {
  EngineCache,
  initSync,
  Refusal,
  type SideBySideResult,
} from '../wasm/transit_braid_wasm.js';

beforeAll(async () => {
  const bytes = await readFile(new URL('../wasm/transit_braid_wasm_bg.wasm', import.meta.url));
  initSync({ module: bytes });
  engine = new EngineCache();
});

/** One cache for the tests' calls: it carries from call to call, as an editor session's does. */
let engine: EngineCache;

/** The engine's one entry point, on the tests' cache. */
const engineSideBySide = (...args: Parameters<EngineCache['sideBySide']>): SideBySideResult =>
  engine.sideBySide(...args);

/** A result's arrays, each taken out once (the result is freed). */
function taken(out: SideBySideResult) {
  try {
    return {
      coords: out.takeCoords(),
      starts: out.takeStarts(),
      bars: out.takeBars(),
      dropped: out.takeDropped(),
    };
  } finally {
    out.free();
  }
}

/** The pipeline without route keys (matched by place in the list), uncapped. */
function sideBySide(
  coords: Float64Array,
  starts: Uint32Array,
  stations: Float64Array,
  stationGroups: Uint32Array,
  zoom: number | undefined,
  metersPerPixel: number | undefined,
  lineWidth: number,
  spacing: number,
) {
  return taken(
    engineSideBySide(
      coords,
      starts,
      new Uint32Array(),
      stations,
      stationGroups,
      zoom,
      metersPerPixel,
      lineWidth,
      spacing,
      undefined,
    ),
  );
}

/** The refusal `call` throws, as plain data; fails the test if it draws instead. */
function refusalOf(call: () => unknown) {
  try {
    call();
  } catch (error) {
    if (!(error instanceof Refusal)) throw error;
    const { code, reason, route, point, station } = error;
    error.free();
    return { code, reason, route, point, station };
  }
  throw new Error('expected a refusal');
}

/** The refusal of `sideBySide` on these routes, without stations. */
const refused = (coords: number[], starts: number[], zoom?: number, mpp?: number) =>
  refusalOf(() =>
    sideBySide(
      new Float64Array(coords),
      new Uint32Array(starts),
      new Float64Array(),
      new Uint32Array(),
      zoom,
      mpp,
      4,
      1,
    ),
  );

describe('WebAssembly bridge (Node)', () => {
  it('runs the whole pipeline: one side-by-side line per input line', () => {
    // Two copies of a 400 m street running east at the equator, 1 m per pixel, pitch 4 + 1 px.
    const street = [0, 0, 0.0036, 0];
    const out = sideBySide(
      new Float64Array([...street, ...street]),
      new Uint32Array([0, 2]),
      // A station on the street and one 500 m north of it.
      new Float64Array([0.0018, 0.00001, 0.0018, 0.0045]),
      new Uint32Array(),
      undefined,
      1,
      4,
      1,
    );
    expect(out.coords).toBeInstanceOf(Float64Array);
    expect([...out.starts]).toEqual([0, 2]);
    const [a, b] = [out.coords[1]!, out.coords[5]!];
    expect(a + b).toBeCloseTo(0, 12);
    expect(Math.abs(a) * 111_320).toBeCloseTo(2.5, 2); // 2.5 m either side
    // The first station's bar spans both lines, square to the street; the second has none.
    const bars = [...out.bars];
    expect(bars).toHaveLength(8);
    expect(bars[0]).toBeCloseTo(0.0018, 9);
    expect(bars[2]).toBeCloseTo(0.0018, 9);
    expect(Math.abs((bars[3] ?? 0) - (bars[1] ?? 0)) * 111_320).toBeCloseTo(9, 1); // 2 × (2.5 + 2)
    expect(bars.slice(4).every(Number.isNaN)).toBe(true);
  });

  it('caps lines side by side and lists the routes it leaves out', () => {
    // Three copies of one street: with a cap of 2 the third is left out.
    const street = [0, 0, 0.0036, 0];
    const capped = (cap: number) =>
      taken(
        engineSideBySide(
          new Float64Array([...street, ...street, ...street]),
          new Uint32Array([0, 2, 4]),
          new Uint32Array(),
          new Float64Array(),
          new Uint32Array(),
          16,
          undefined,
          4,
          1,
          cap,
        ),
      );
    const two = capped(2);
    expect(two.dropped).toBeInstanceOf(Uint32Array);
    expect([...two.dropped]).toEqual([2]);
    expect([...two.starts]).toEqual([0, 2]);
    const alone = sideBySide(
      new Float64Array([...street, ...street]),
      new Uint32Array([0, 2]),
      new Float64Array(),
      new Uint32Array(),
      16,
      undefined,
      4,
      1,
    );
    expect([...two.coords]).toEqual([...alone.coords]);
    expect([...capped(3).dropped]).toEqual([]);
    expect([...capped(2 ** 32 - 1).dropped]).toEqual([]);
    expect([...alone.dropped]).toEqual([]); // the uncapped call has none
    expect(refusalOf(() => capped(0))).toMatchObject({
      code: 'BAD_OPTION',
      reason: 'invalid settings: max side by side must be 1 or more',
    });
  });

  it('moves each array out of the result once: a second take gives an empty array', () => {
    const street = [0, 0, 0.0036, 0];
    const out = engineSideBySide(
      new Float64Array([...street, ...street]),
      new Uint32Array([0, 2]),
      new Uint32Array(),
      new Float64Array(),
      new Uint32Array(),
      16,
      undefined,
      4,
      1,
      undefined,
    );
    expect(out.takeCoords()).toHaveLength(8);
    expect(out.takeCoords()).toHaveLength(0);
    expect([...out.takeStarts()]).toEqual([0, 2]);
    expect([...out.takeStarts()]).toEqual([]);
    out.free();
  });

  it('lets same-name stations share one bar and checks one group per station', () => {
    const street = [0, 0, 0.0036, 0];
    const call = (groups: number[]) =>
      sideBySide(
        new Float64Array([...street, ...street]),
        new Uint32Array([0, 2]),
        // Two stops 40 m apart along the street.
        new Float64Array([0.0016, 0.00001, 0.00196, -0.00001]),
        new Uint32Array(groups),
        undefined,
        1,
        4,
        1,
      );
    const merged = [...call([1, 1]).bars];
    expect(merged.slice(0, 4)).toEqual(merged.slice(4));
    expect(merged[0]).toBeCloseTo(0.00178, 6);
    const own = [...call([1, 2]).bars];
    expect(own[0]).toBeCloseTo(0.0016, 6);
    expect(own[4]).toBeCloseTo(0.00196, 6);
    expect(refusalOf(() => call([1]))).toMatchObject({
      code: 'BAD_LAYOUT',
      reason: expect.stringContaining('one per station'),
    });
  });

  it('draws the same lines again after a zoom change and back', () => {
    const street = [0, 0, 0.0036, 0];
    const call = (zoom: number) =>
      sideBySide(
        new Float64Array([...street, ...street]),
        new Uint32Array([0, 2]),
        new Float64Array(),
        new Uint32Array(),
        zoom,
        undefined,
        4,
        1,
      );
    const first = [...call(14).coords];
    expect([...call(17).coords]).not.toEqual(first);
    expect([...call(14).coords]).toEqual(first);
  });

  it('gives no lines for no lines', () => {
    const out = sideBySide(
      new Float64Array(),
      new Uint32Array(),
      new Float64Array(),
      new Uint32Array(),
      14,
      undefined,
      4,
      1,
    );
    expect(out.coords.length).toBe(0);
    expect(out.starts.length).toBe(0);
  });

  it('throws a Refusal with a code, the reason, and the route, point or station at fault', () => {
    const none = { route: undefined, point: undefined, station: undefined };
    expect(refused([0, 0, 1, 0], [0], 14, 1)).toEqual({
      code: 'BAD_OPTION',
      reason: 'give exactly one of zoom and metersPerPixel',
      ...none,
    });
    expect(refused([0, 0, 1, 0], [0])).toMatchObject({ code: 'BAD_OPTION', ...none });
    expect(refused([0, 0, 200, 0], [0], 14)).toEqual({
      code: 'BAD_COORDINATE',
      reason: 'needs a finite longitude from -180 to 180 and latitude from -85.05 to 85.05',
      route: 0,
      point: 1,
      station: undefined,
    });
    expect(refused([0, 0, 0, 0], [0], 14)).toEqual({
      code: 'TOO_FEW_POINTS',
      reason: 'needs at least two distinct points',
      route: 0,
      point: undefined,
      station: undefined,
    });
    expect(refused([0, 0, 1, 0], [1], 14)).toMatchObject({
      code: 'BAD_LAYOUT',
      reason: 'starts must begin at 0, got 1',
      ...none,
    });
    const many = Array.from({ length: 71 }, (_, i) => i * 2);
    expect(
      refused(
        Array.from({ length: 284 }, () => 0),
        many,
        14,
      ),
    ).toMatchObject({ code: 'TOO_MANY_ROUTES', reason: expect.stringContaining('at most 70') });
    const station = refusalOf(() =>
      sideBySide(
        new Float64Array([0, 0, 0.001, 0]),
        new Uint32Array([0]),
        new Float64Array([0, 0, 0, 95]),
        new Uint32Array(),
        14,
        undefined,
        4,
        1,
      ),
    );
    expect(station).toMatchObject({ code: 'BAD_STATION', station: 1, route: undefined });
  });

  it('draws a route of 10,000 points in one call', () => {
    // 10 km east at the equator, a point every metre.
    const n = 10_000;
    const coords = new Float64Array(2 * n);
    for (let i = 0; i < n; i++) coords.set([i / 111_320, 0], 2 * i);
    const out = sideBySide(
      coords,
      new Uint32Array([0]),
      new Float64Array(),
      new Uint32Array(),
      undefined,
      1,
      4,
      1,
    );
    expect([...out.starts]).toEqual([0]);
    expect(out.coords.length).toBeGreaterThanOrEqual(4);
    expect(out.coords.every(Number.isFinite)).toBe(true);
  });
});
