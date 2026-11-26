import { InputError } from './errors.ts';
import type { StationsInput } from './stations.ts';

/**
 * Options a caller may pass to `sideBySide()`. The map scale is required: give exactly one of
 * `zoom` and `metersPerPixel` (both, or neither, is an `InputError` with code `BAD_OPTION`).
 * Everything else is optional.
 */
export interface SideBySideOptions {
  /**
   * The map's zoom level, 0 to 24, in **512-pixel tiles**: the zoom of MapLibre GL, Mapbox GL
   * and deck.gl, as their map reports it. Maps with 256-pixel tiles (Leaflet, OpenLayers,
   * Google Maps) are one level higher for the same scale: pass their zoom − 1. Give this or
   * `metersPerPixel`, not both.
   */
  zoom?: number;
  /**
   * Ground meters per screen pixel where the routes are (at the middle of their bounding box),
   * above 0. Use it for any map without 512-pixel tile zooms. On a Web Mercator map at zoom `z`
   * with `t`-pixel tiles it is `2π × 6378137 × cos(latitude) / (t × 2^z)`. Give this or `zoom`,
   * not both.
   */
  metersPerPixel?: number;
  /** Width of each line in pixels. Default 4 (also when plain JavaScript passes `null`). */
  lineWidth?: number;
  /** Gap between neighboring lines in pixels. Default 1 (also when plain JavaScript passes
   * `null`). `zoom` has no default, so a `null` zoom is an error. */
  spacing?: number;
  /** Stations: a GeoJSON FeatureCollection of Points (optional `id`, properties such as
   * `name`); each one near a route gets a bar across the bundle in the result's `stations`. */
  stations?: StationsInput | null | undefined;
  /**
   * Lane cap: at most this many lines side by side on any street. Routes are kept in the
   * order given while every street they run along has room; the others are left out whole and
   * listed in the result's `dropped`, and the kept routes are drawn on their own. The same
   * routes are left out at every zoom. A whole number, 1 or more; left out (or `null`): no cap.
   */
  maxSideBySide?: number | null | undefined;
}

/** Options after defaults are filled in and values are checked. */
export interface ResolvedOptions {
  /** Exactly one way of giving the scale. */
  scale: { kind: 'zoom'; zoom: number } | { kind: 'metersPerPixel'; metersPerPixel: number };
  lineWidth: number;
  spacing: number;
  /** The lane cap, or `undefined` for none; at most 2³² − 1 (any cap of 70 or more drops none). */
  maxSideBySide: number | undefined;
}

/** Largest cap passed on: the WASM call takes a 32-bit whole number. */
const MAX_CAP = 0xffff_ffff;

/** Default line width in pixels. */
export const DEFAULT_LINE_WIDTH = 4;
/** Default gap between lines in pixels. */
export const DEFAULT_SPACING = 1;

/**
 * Fills in defaults and checks every option, so the rest of the library can trust them.
 * Throws an `InputError` (code `BAD_OPTION`, `option` naming it) with a readable message on bad
 * input.
 */
export function resolveOptions(options: SideBySideOptions): ResolvedOptions {
  // Plain-JS callers can pass anything; a missing options object is a clear error, not a crash.
  if (typeof options !== 'object' || options === null) {
    throw badOption('options', '`options` must be an object, e.g. `{ zoom: 14 }`.');
  }
  const { zoom, metersPerPixel } = options;
  // The option to fix: the zoom when neither is given, the second one when both are.
  if (zoom === undefined && metersPerPixel === undefined) {
    throw badOption('zoom', 'Give exactly one of `zoom` or `metersPerPixel`: neither was given.');
  }
  if (zoom !== undefined && metersPerPixel !== undefined) {
    throw badOption(
      'metersPerPixel',
      'Give exactly one of `zoom` or `metersPerPixel`: both were given.',
    );
  }

  let scale: ResolvedOptions['scale'];
  if (zoom !== undefined) {
    requireFinite('zoom', zoom);
    if (zoom < 0 || zoom > 24) throw badOption('zoom', '`zoom` must be between 0 and 24.');
    scale = { kind: 'zoom', zoom };
  } else {
    requirePositive('metersPerPixel', metersPerPixel);
    scale = { kind: 'metersPerPixel', metersPerPixel };
  }

  const lineWidth = options.lineWidth ?? DEFAULT_LINE_WIDTH;
  const spacing = options.spacing ?? DEFAULT_SPACING;
  requirePositive('lineWidth', lineWidth);
  requireFinite('spacing', spacing);
  if (spacing < 0) throw badOption('spacing', '`spacing` must be 0 or more.');

  const cap = options.maxSideBySide ?? undefined;
  if (cap !== undefined) {
    requireFinite('maxSideBySide', cap);
    if (!Number.isInteger(cap) || cap < 1) {
      throw badOption('maxSideBySide', '`maxSideBySide` must be a whole number, 1 or more.');
    }
  }
  const maxSideBySide = cap === undefined ? undefined : Math.min(cap, MAX_CAP);

  return { scale, lineWidth, spacing, maxSideBySide };
}

function requireFinite(name: string, value: unknown): asserts value is number {
  // `typeof` is for the type checker: Number.isFinite already rejects non-numbers.
  if (typeof value !== 'number' || !Number.isFinite(value)) {
    throw badOption(name, `\`${name}\` must be a finite number.`);
  }
}

function requirePositive(name: string, value: unknown): asserts value is number {
  requireFinite(name, value);
  if (value <= 0) throw badOption(name, `\`${name}\` must be greater than 0.`);
}

/** The error for a bad `option`. */
function badOption(option: string, message: string): InputError {
  return new InputError('BAD_OPTION', message, { option });
}
