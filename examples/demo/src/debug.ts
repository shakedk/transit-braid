// What the demo exposes on `window.braidDebug` so end-to-end tests can inspect it.
// The `declare global` below applies project-wide; no import needed.
import type { MapboxOverlay } from '@deck.gl/mapbox';
import type { Map as MapLibreMap } from 'maplibre-gl';

import type { LngLat } from './project.ts';

export interface BraidDebug {
  map: MapLibreMap;
  overlay: MapboxOverlay;
  /** True once the map has loaded and something was drawn. */
  ready: boolean;
  /** The network shown (its id in `networks.ts`). */
  network: string;
  /** The "Side by side" box: lines from `sideBySide()`, or the routes as given. */
  sideBySide: boolean;
  /** The routes' ids, in order. */
  routes: string[];
  /** Changes so far (each edit, toggle or zoom asks for a draw). */
  runs: number;
  /** `sideBySide()` calls made (changes that arrive during a call wait; only the newest runs). */
  calls: number;
  /** The change whose result is on screen (equals `runs` once everything is drawn). */
  shown: number;
  /** Moves point `index` of a route, as dragging its marker does (for tests of rapid edits). */
  move: (routeId: string, index: number, to: LngLat) => void;
  /** The route whose points can be dragged, or null. */
  editing: string | null;
  /** Points clicked so far while adding a route, or null when not adding. */
  drafting: number | null;
  /** The status line. */
  status: string;
  /** For each drawn line, a point that lies on it (middle of its first segment). */
  probes: Record<string, LngLat>;
  /** Station bars drawn. */
  bars: number;
  /** Direction marks drawn along the lines. */
  marks: number;
  /** Routes the lane cap left out of the last draw. */
  dropped: string[];
}

declare global {
  interface Window {
    braidDebug?: BraidDebug;
  }
}
