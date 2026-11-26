// Direction marks along drawn lines: a small arrow every so many meters, pointing the way the
// route runs, so each direction of a route reads as its own line. Pure, so it is tested in Node;
// main.ts draws the marks with deck.gl's TextLayer.
import type { LngLat } from './project.ts';

/** One mark: where it sits, and its direction in degrees, counter-clockwise from east. */
export interface Mark {
  position: LngLat;
  angle: number;
}

// Meters per degree, close enough to space marks on a city map.
const M_PER_DEG_LAT = 110_540;
const M_PER_DEG_LNG_AT_EQUATOR = 111_320;
const EARTH_CIRCUMFERENCE_M = 40_075_016.686;

/** Ground meters per screen pixel at `zoom` (512-pixel tiles, as MapLibre counts) and `lat`. */
export function metersPerPixel(zoom: number, lat: number): number {
  return (EARTH_CIRCUMFERENCE_M * Math.cos((lat * Math.PI) / 180)) / (512 * 2 ** zoom);
}

/**
 * Marks every `spacing` meters along `path`, the first half a spacing from its start, each turned
 * the way its segment runs. None for a path with fewer than two points or a spacing that isn't a
 * positive number; segments of zero length are skipped.
 */
export function marksAlong(path: readonly LngLat[], spacing: number): Mark[] {
  if (!(spacing > 0) || !Number.isFinite(spacing)) return [];
  const marks: Mark[] = [];
  let next = spacing / 2;
  let walked = 0;
  for (let i = 1; i < path.length; i++) {
    const a = path[i - 1];
    const b = path[i];
    if (!a || !b) continue;
    const kx = M_PER_DEG_LNG_AT_EQUATOR * Math.cos((((a[1] + b[1]) / 2) * Math.PI) / 180);
    const dx = (b[0] - a[0]) * kx;
    const dy = (b[1] - a[1]) * M_PER_DEG_LAT;
    const length = Math.hypot(dx, dy);
    if (!(length > 0)) continue;
    const angle = (Math.atan2(dy, dx) * 180) / Math.PI;
    while (next <= walked + length) {
      const t = (next - walked) / length;
      marks.push({ position: [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t], angle });
      next += spacing;
    }
    walked += length;
  }
  return marks;
}
