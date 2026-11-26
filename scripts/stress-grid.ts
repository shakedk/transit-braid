// The 70-route stress grid: the most one call takes. A made-up downtown: a 12 × 12
// street grid, 200 m blocks, and 70 bus routes that wander along it from a fixed seed, so many
// share streets, cross, join and part. Synthetic, MIT, committed.
//   node scripts/stress-grid.ts   writes fixtures/stress/stress-grid.geojson
// The test (stress-grid.test.ts) checks the committed file is exactly what this writes.
import { writeFileSync } from 'node:fs';
import { join } from 'node:path';

/** Streets each way, and the block length in meters. */
const STREETS = 12;
const BLOCK_M = 200;
const ROUTES = 70;
/** Each route is shifted by up to this many meters each way (real routes on one street never
 * have identical coordinates; well inside the 5 m merge distance). */
const JITTER_M = 1.5;
const SEED = 2026;
const COLORS = ['#e4002b', '#0072ce', '#00a651', '#ff8200', '#7d2b8b', '#00a3ad', '#a6192e'];

/** Small seeded random numbers (mulberry32): the same fixture on every machine. */
export function random(seed: number): () => number {
  let s = seed >>> 0;
  return () => {
    s = (s + 0x6d_2b_79_f5) >>> 0;
    let t = s;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4_294_967_296;
  };
}

type Node = [number, number];

const STEPS: Node[] = [
  [1, 0],
  [0, 1],
  [-1, 0],
  [0, -1],
];

/**
 * One route: a walk along the grid from a random corner, 10 to 22 blocks, never visiting a
 * corner twice, going straight on more often than turning (like a bus route), as grid corners.
 */
function walk(rand: () => number): Node[] {
  const pick = <T>(items: readonly T[]): T => {
    const item = items[Math.floor(rand() * items.length)];
    if (item === undefined) throw new Error('nothing to pick from');
    return item;
  };
  const inside = ([x, y]: Node) => x >= 0 && y >= 0 && x < STREETS && y < STREETS;
  const start: Node = [Math.floor(rand() * STREETS), Math.floor(rand() * STREETS)];
  const blocks = 10 + Math.floor(rand() * 13);
  const path: Node[] = [start];
  const seen = new Set([start.join(',')]);
  let heading = pick(STEPS);
  for (let k = 0; k < blocks; k++) {
    const here = path.at(-1) ?? start;
    const next = (d: Node): Node => [here[0] + d[0], here[1] + d[1]];
    const open = STEPS.filter((d) => inside(next(d)) && !seen.has(next(d).join(',')));
    if (open.length === 0) break;
    const straight = open.find((d) => d[0] === heading[0] && d[1] === heading[1]);
    heading = straight !== undefined && rand() < 0.6 ? straight : pick(open);
    const n = next(heading);
    path.push(n);
    seen.add(n.join(','));
  }
  return path;
}

/** Keeps only the corners where the route turns (and its two ends). */
function turnsOnly(path: Node[]): Node[] {
  return path.filter((p, i) => {
    const a = path[i - 1];
    const b = path[i + 1];
    if (a === undefined || b === undefined) return true;
    return (p[0] - a[0]) * (b[1] - p[1]) - (p[1] - a[1]) * (b[0] - p[0]) !== 0;
  });
}

/** The fixture as JSON text, ending in a newline. */
export function stressGrid(): string {
  const rand = random(SEED);
  const features = [];
  for (let r = 0; r < ROUTES; r++) {
    let path = walk(rand);
    while (path.length < 3) path = walk(rand);
    const dx = Math.round((rand() * 2 - 1) * JITTER_M * 10) / 10;
    const dy = Math.round((rand() * 2 - 1) * JITTER_M * 10) / 10;
    features.push({
      type: 'Feature',
      properties: { line: `R${String(r + 1).padStart(2, '0')}`, color: COLORS[r % COLORS.length] },
      geometry: {
        type: 'LineString',
        coordinates: turnsOnly(path).map(([x, y]) => [x * BLOCK_M + dx, y * BLOCK_M + dy]),
      },
    });
  }
  const fixture = {
    type: 'FeatureCollection',
    braid: {
      name: 'stress-grid',
      description: `${String(ROUTES)} routes wandering a ${String(STREETS)} × ${String(STREETS)} street grid of ${String(BLOCK_M)} m blocks (the most a call takes).`,
      units: 'meters',
      license: 'MIT',
      attribution: 'transit-braid authors (synthetic, scripts/stress-grid.ts)',
      expect:
        'Every shared street is one bundle; lines join and part at corners; nothing is lost or drawn twice.',
    },
    features,
  };
  return `${JSON.stringify(fixture, null, 2)}\n`;
}

if (import.meta.main) {
  const out = join(import.meta.dirname, '..', 'fixtures', 'stress', 'stress-grid.geojson');
  writeFileSync(out, stressGrid());
  console.log(`wrote ${out}`);
}
