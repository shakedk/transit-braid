// Colors for BraidLayer: a route's color from its GeoJSON properties.

/** An RGB or RGBA color, each part 0–255 (deck.gl's `Color`). */
export type Rgba = [number, number, number] | [number, number, number, number];

/** Gray, for a route with no usable color. */
export const DEFAULT_COLOR: Rgba = [128, 128, 128];

/**
 * The default `getColor`: `properties.color` as `#rgb` or `#rrggbb` (case-insensitive). A route
 * without one, or with anything else there (not a string, not hex, the wrong length), is gray.
 */
export function colorOf(route: { properties?: unknown }): Rgba {
  const properties = route.properties;
  if (typeof properties !== 'object' || properties === null) return DEFAULT_COLOR;
  const value = 'color' in properties ? properties.color : undefined;
  if (typeof value !== 'string') return DEFAULT_COLOR;
  const hex = /^#([0-9a-f]{3}|[0-9a-f]{6})$/i.exec(value.trim())?.[1];
  if (hex === undefined) return DEFAULT_COLOR;
  // Hex digits are ASCII, so splitting by character is safe.
  const full = hex.length === 3 ? hex.replaceAll(/./g, (c) => c + c) : hex;
  const part = (i: number): number => Number.parseInt(full.slice(i, i + 2), 16);
  return [part(0), part(2), part(4)];
}
