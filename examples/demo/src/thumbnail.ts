// Small pictures of a network's routes, as given, for the Examples page: each path fitted into a
// box, keeping its shape. Pure, so it is tested in Node.
import type { LngLat } from './project.ts';

/** A point in the picture, in pixels from its top-left corner. */
export type Pixel = [number, number];

/**
 * Fits `paths` (longitude, latitude) into a `width` × `height` box with `pad` pixels on every
 * side, north up, keeping their shape (a degree of longitude is shortened by the cosine of the
 * latitude). Everything sits in the middle of the box; a single point lands at its center. No
 * points, no paths.
 */
export function fitPaths(
  paths: readonly (readonly LngLat[])[],
  width: number,
  height: number,
  pad: number,
): Pixel[][] {
  const all = paths.flat();
  if (all.length === 0) return paths.map(() => []);
  const lats = all.map((p) => p[1]);
  const midLat = (Math.min(...lats) + Math.max(...lats)) / 2;
  const kx = Math.cos((midLat * Math.PI) / 180);
  const flat = (p: LngLat): Pixel => [p[0] * kx, -p[1]];
  const xs = all.map((p) => flat(p)[0]);
  const ys = all.map((p) => flat(p)[1]);
  const [minX, maxX, minY, maxY] = [
    Math.min(...xs),
    Math.max(...xs),
    Math.min(...ys),
    Math.max(...ys),
  ];
  const spanX = maxX - minX;
  const spanY = maxY - minY;
  const roomX = Math.max(width - 2 * pad, 0);
  const roomY = Math.max(height - 2 * pad, 0);
  const scale = Math.min(
    spanX > 0 ? roomX / spanX : Infinity,
    spanY > 0 ? roomY / spanY : Infinity,
  );
  const s = Number.isFinite(scale) ? scale : 0;
  const offX = (width - spanX * s) / 2;
  const offY = (height - spanY * s) / 2;
  return paths.map((path) =>
    path.map((p): Pixel => {
      const [x, y] = flat(p);
      return [offX + (x - minX) * s, offY + (y - minY) * s];
    }),
  );
}
