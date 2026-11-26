// The golden harness in WebAssembly: every committed fixture through `sideBySide()` must give
// exactly the digests in fixtures/golden.json, which `cargo test -p transit-braid-testkit --test
// golden` checks natively, so native and WebAssembly draw the same bits. The inputs, runs and
// digest are the Rust harness's (crates/transit-braid-testkit/src/golden.rs); keep them in step.
// Each run is a fresh session (`createSession()`): its own cache, so the zooms and the edit script
// go through one cache as the Rust harness's do, and a session's edits are proven to draw what the
// cached path always drew. Needs `pnpm build:wasm` first (pnpm check does this).
import { readdirSync, readFileSync } from 'node:fs';

import { describe, expect, it } from 'vitest';

import {
  createSession,
  type Position,
  type RoutesInput,
  type SideBySideOptions,
  type SideBySideResult,
  type StationsInput,
} from './node.ts';

const FIXTURES = new URL('../../../fixtures/', import.meta.url);
/** The committed fixture folders (the Rust harness digests the same). */
const FOLDERS = ['synthetic', 'bus', 'stress'];

/** The Rust harness's settings (golden.rs): `sideBySide()`'s default widths. */
const LINE_WIDTH = 4;
const SPACING = 1;
/** The whole zooms, then the fractional ones (golden.rs `ZOOMS`, `FRACTIONAL_ZOOMS`). */
const ZOOMS = [10, 11, 12, 13, 14, 15, 16, 17, 18, 19];
const FRACTIONAL_ZOOMS = [12.5, 15.37, 17.75];
const CAPS = [1, 2, 3];
const CAP_ZOOM = 15;
const EDIT_ZOOM = 15;
const EDIT_ZOOM_AFTER = 17;
const MOVE_BY: [number, number] = [0.0001, 0.000_05];
/** Meters per degree for fixtures in meters, placed around 0°, 0°. */
const PLACE_M_PER_DEG = 111_319.490_793_273_57;
/** The most stations one call takes. */
const MAX_STATIONS = 5000;

interface Pair {
  lines: string;
  bars: string;
}

interface GoldenRecord {
  file: string;
  stations: number;
  zooms: Record<string, Pair>;
  caps: Record<string, Pair>;
  edits: Record<string, Pair>;
}

interface Fixture {
  braid: {
    units: 'meters' | 'degrees';
    stations?: { name?: string; at: [number, number] }[];
  };
  features: { properties: { line: string }; geometry: { coordinates: [number, number][] } }[];
}

function readJson(url: URL): unknown {
  return JSON.parse(readFileSync(url, 'utf8'));
}

// oxlint-disable-next-line typescript/no-unsafe-type-assertion -- written by the Rust harness, which checks its shape
const golden = readJson(new URL('golden.json', FIXTURES)) as {
  fixtures: Record<string, GoldenRecord>;
};

/** 64-bit FNV-1a in two 32-bit halves, so every step is exact in a JS number. */
class Fnv64 {
  private hi = 0xcb_f2_9c_e4;
  private lo = 0x84_22_23_25;
  private readonly scratch = new DataView(new ArrayBuffer(8));

  bytes(bytes: Uint8Array): void {
    for (const byte of bytes) {
      const lo = (this.lo ^ byte) >>> 0;
      // × 0x100000001b3 = × 0x1b3, plus the number shifted left by 40 bits.
      const low = lo * 0x1b3;
      const carry = Math.floor(low / 0x1_00_00_00_00);
      this.hi = (this.hi * 0x1b3 + carry + ((lo << 8) >>> 0)) % 0x1_00_00_00_00;
      this.lo = low % 0x1_00_00_00_00;
    }
  }

  /** A count or length as 8 little-endian bytes. */
  count(n: number): void {
    this.scratch.setBigUint64(0, BigInt(n), true);
    this.bytes(new Uint8Array(this.scratch.buffer));
  }

  /** A coordinate's exact bits, little-endian. */
  f64(x: number): void {
    this.scratch.setFloat64(0, x, true);
    this.bytes(new Uint8Array(this.scratch.buffer));
  }

  /** A text: its UTF-8 length, then its bytes. */
  text(s: string): void {
    const utf8 = new TextEncoder().encode(s);
    this.count(utf8.length);
    this.bytes(utf8);
  }

  hex(): string {
    return this.hi.toString(16).padStart(8, '0') + this.lo.toString(16).padStart(8, '0');
  }
}

/** The two digests of one `sideBySide()` result, for `stations` stations (golden.rs `digest`). */
function digest(out: SideBySideResult, stations: number): Pair {
  const lines = new Fnv64();
  lines.count(out.lines.features.length);
  for (const line of out.lines.features) {
    lines.text(String(line.id));
    lines.count(line.geometry.coordinates.length);
    for (const [x = NaN, y = NaN] of line.geometry.coordinates) {
      lines.f64(x);
      lines.f64(y);
    }
  }
  lines.count(out.dropped.length);
  for (const id of out.dropped) lines.text(String(id));

  const barOf = new Map<unknown, Position[]>(
    out.stations.features.map((f) => [f.id, f.geometry.coordinates]),
  );
  const bars = new Fnv64();
  bars.count(stations);
  for (let i = 0; i < stations; i++) {
    const bar = barOf.get(i);
    if (bar === undefined) {
      bars.bytes(new Uint8Array([0]));
      continue;
    }
    bars.bytes(new Uint8Array([1]));
    for (const [x = NaN, y = NaN] of bar) {
      bars.f64(x);
      bars.f64(y);
    }
  }
  return { lines: lines.hex(), bars: bars.hex() };
}

interface Input {
  ids: string[];
  lines: Position[][];
  stations: StationsInput;
}

/** A fixture's routes and stations in `[lng, lat]`, as golden.rs `input` makes them. */
function input(fixture: Fixture): Input {
  const place = ([x, y]: [number, number]): Position =>
    fixture.braid.units === 'meters' ? [x / PLACE_M_PER_DEG, y / PLACE_M_PER_DEG] : [x, y];
  const lines = fixture.features.map((f) => f.geometry.coordinates.map(place));
  const stops = fixture.braid.stations ?? [];
  let points: { at: Position; name?: string | undefined }[];
  if (stops.length > 0) {
    points = stops.map((s) => ({ at: place(s.at), name: s.name }));
  } else {
    const mids = lines.flatMap((l) =>
      l.slice(1).map((b, k) => {
        const a = l[k] ?? b;
        return {
          at: [((a[0] ?? NaN) + (b[0] ?? NaN)) * 0.5, ((a[1] ?? NaN) + (b[1] ?? NaN)) * 0.5],
        };
      }),
    );
    const stride = Math.max(1, Math.ceil(mids.length / MAX_STATIONS));
    points = mids.filter((_, i) => i % stride === 0);
  }
  return {
    ids: fixture.features.map((f) => f.properties.line),
    lines,
    stations: {
      type: 'FeatureCollection',
      features: points.map((p, i) => ({
        type: 'Feature',
        id: i,
        properties: p.name === undefined ? null : { name: p.name },
        geometry: { type: 'Point', coordinates: p.at },
      })),
    },
  };
}

/** The routes `picked` (fixture indices, in that order), with their lines taken from `lines`. */
function routes(ids: string[], lines: Position[][], picked: number[]): RoutesInput {
  return {
    type: 'FeatureCollection',
    features: picked.map((i) => ({
      type: 'Feature',
      id: ids[i],
      properties: null,
      geometry: { type: 'LineString', coordinates: lines[i] ?? [] },
    })),
  };
}

/** Every digest of the fixture `file` (relative to fixtures/), as golden.rs `record` makes them. */
async function record(file: string): Promise<GoldenRecord> {
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- a fixture file, checked by the fixtures crate
  const fixture = readJson(new URL(file, FIXTURES)) as Fixture;
  const { ids, lines, stations } = input(fixture);
  const all = [...ids.keys()];
  const count = stations.features.length;
  const options = (zoom: number, more: Partial<SideBySideOptions> = {}): SideBySideOptions => ({
    zoom,
    lineWidth: LINE_WIDTH,
    spacing: SPACING,
    stations,
    ...more,
  });
  /** `steps` in turn through one fresh session: each call sees the cache the last one left. */
  const inTurn = async (steps: [string, RoutesInput, SideBySideOptions][]) => {
    const session = createSession();
    try {
      const out: Record<string, Pair> = {};
      for (const [key, picked, opts] of steps) {
        // oxlint-disable-next-line eslint/no-await-in-loop -- in turn on purpose: one cache
        out[key] = digest(await session.sideBySide(picked, opts), count);
      }
      return out;
    } finally {
      session.close();
    }
  };

  // Zooms in turn through one cache: zoom-only reruns, which the Rust harness proves equal to a
  // fresh run at each zoom. A key is the zoom as JavaScript writes it, the same as Rust ("15.37").
  const zooms = inTurn(
    [...ZOOMS, ...FRACTIONAL_ZOOMS].map((z) => [String(z), routes(ids, lines, all), options(z)]),
  );
  const caps = Promise.all(
    CAPS.map(async (cap) => {
      const one = await inTurn([
        [String(cap), routes(ids, lines, all), options(CAP_ZOOM, { maxSideBySide: cap })],
      ]);
      return Object.entries(one);
    }),
  );

  const moved = lines.map((l) => l.map((p) => [...p]));
  const first = moved[0] ?? [];
  const k = Math.floor(first.length / 2);
  const [x = NaN, y = NaN] = first[k] ?? [];
  first[k] = [x + MOVE_BY[0], y + MOVE_BY[1]];
  const reversed = all.toReversed();
  const edits = inTurn([
    ['1-full', routes(ids, lines, all), options(EDIT_ZOOM)],
    ['2-move', routes(ids, moved, all), options(EDIT_ZOOM)],
    ['3-remove', routes(ids, moved, all.slice(0, -1)), options(EDIT_ZOOM)],
    ['4-add', routes(ids, moved, all), options(EDIT_ZOOM)],
    ['5-reverse', routes(ids, moved, reversed), options(EDIT_ZOOM)],
    ['6-zoom', routes(ids, moved, reversed), options(EDIT_ZOOM_AFTER)],
  ]);

  return {
    file,
    stations: count,
    zooms: await zooms,
    caps: Object.fromEntries((await caps).flat()),
    edits: await edits,
  };
}

/** The 64-bit FNV-1a digest of a text's UTF-8 bytes. */
function fnv(s: string): string {
  const h = new Fnv64();
  h.bytes(new TextEncoder().encode(s));
  return h.hex();
}

describe('golden digests in WebAssembly', () => {
  it('uses 64-bit FNV-1a: the published test vectors', () => {
    expect(fnv('')).toBe('cbf29ce484222325');
    expect(fnv('a')).toBe('af63dc4c8601ec8c');
    expect(fnv('foobar')).toBe('85944171f73967e8');
  });

  it('has a record for every committed fixture, named after its file, and nothing else', () => {
    const files = FOLDERS.flatMap((folder) =>
      readdirSync(new URL(`${folder}/`, FIXTURES))
        .filter((f) => f.endsWith('.geojson'))
        .map((f) => `${folder}/${f}`),
    );
    const records = Object.entries(golden.fixtures);
    expect(records.map(([, r]) => r.file).toSorted()).toEqual(files.toSorted());
    for (const [name, r] of records) expect(r.file).toMatch(new RegExp(`/${name}\\.geojson$`));
  });

  it.each(Object.entries(golden.fixtures))(
    '%s draws its golden digests, to the bit',
    async (_name, expected) => {
      expect(await record(expected.file)).toEqual(expected);
    },
    60_000,
  );
});
