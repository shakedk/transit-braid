// transit-braid-deck's demo page: three routes sharing a street, drawn by BraidLayer on
// deck.gl's own canvas (no base map, so the e2e test sees exact colors). Pan and zoom to watch
// the lines keep their slots; `window.deckDebug` is the e2e test's handle.

import { Deck, type MapViewState } from '@deck.gl/core';
import type { LineFeature, RouteId, RoutesInput, StationsInput } from 'transit-braid';
import { BraidLayer } from 'transit-braid-deck';

/** A street running east through Tel Aviv's center, shared by three routes. */
const W = 34.774;
const E = 34.786;
const LAT = 32.0765;

const ROUTES: RoutesInput = {
  type: 'FeatureCollection',
  features: [
    {
      type: 'Feature',
      id: 'red',
      properties: { color: '#e4002b' },
      geometry: {
        type: 'LineString',
        coordinates: [
          [W, LAT],
          [E, LAT],
        ],
      },
    },
    {
      type: 'Feature',
      id: 'blue',
      properties: { color: '#0072ce' },
      geometry: {
        type: 'LineString',
        coordinates: [
          [W, LAT],
          [E, LAT],
        ],
      },
    },
    {
      type: 'Feature',
      id: 'green',
      properties: { color: '#00a651' },
      geometry: {
        type: 'LineString',
        coordinates: [
          [W, LAT],
          [0.5 * (W + E), LAT],
          [0.5 * (W + E), LAT + 0.006],
        ],
      },
    },
  ],
};

const STATIONS: StationsInput = {
  type: 'FeatureCollection',
  features: [
    {
      type: 'Feature',
      id: 'stop',
      properties: { name: 'Center' },
      geometry: { type: 'Point', coordinates: [W + 0.25 * (E - W), LAT] },
    },
  ],
};

const INITIAL: MapViewState = { longitude: 0.5 * (W + E), latitude: LAT, zoom: 16 };

const status = document.getElementById('status');
/** The route the last click picked, or null. */
let picked: RouteId | null = null;

/** A drawn line, as picking gives it: the output feature with the route's id and properties. */
function isLine(object: unknown): object is LineFeature {
  if (typeof object !== 'object' || object === null) return false;
  if (!('id' in object) || !('geometry' in object)) return false;
  const geometry = object.geometry;
  return (
    typeof geometry === 'object' &&
    geometry !== null &&
    'type' in geometry &&
    geometry.type === 'LineString'
  );
}
const errors: string[] = [];
let routes: RoutesInput = ROUTES;
/** The lane cap, null for none; and every list `onDropped` reported, in order. */
let cap: number | null = null;
const droppedReports: RouteId[][] = [];

const layer = (): BraidLayer =>
  new BraidLayer({
    id: 'braid',
    data: routes,
    stations: STATIONS,
    lineWidth: 6,
    spacing: 2,
    maxSideBySide: cap,
    onDropped: (ids) => droppedReports.push(ids),
    pickable: true,
    autoHighlight: true,
    highlightColor: [255, 200, 0, 255],
  });

const deck = new Deck({
  initialViewState: INITIAL,
  controller: true,
  layers: [layer()],
  // Hovering a line names its route; clicking it picks the route.
  getTooltip: ({ object }) => (isLine(object) ? `Route ${String(object.id)}` : null),
  onClick: ({ object }) => {
    picked = isLine(object) ? object.id : null;
  },
  onError: (error) => {
    errors.push(error.message);
    if (status) status.textContent = `Error: ${error.message}`;
  },
});

/** The layer deck.gl is drawing now (its state holds the last result). */
function current(): BraidLayer | undefined {
  const [first] = deck.props.layers;
  return first instanceof BraidLayer ? first : undefined;
}

/** The e2e test's handle on the page. */
interface DeckDebug {
  deck: Deck;
  errors: string[];
  zoomShown: () => number | null;
  lines: () => number;
  setRoutes: (next: RoutesInput) => void;
  /** Sets the layer's lane cap; null for none. */
  setCap: (next: number | null) => void;
  /** Every list of left-out routes `onDropped` reported, in order. */
  dropped: () => RouteId[][];
  /** The ids of the lines drawn now, in order. */
  drawnIds: () => RouteId[];
  zoomTo: (zoom: number) => void;
  /** The route the last click picked, or null. */
  picked: () => RouteId | null;
  /** Where route `id`'s drawn line is on screen, `t` of the way along its first segment. */
  screenOf: (id: RouteId, t: number) => { x: number; y: number } | null;
}

const debug: DeckDebug = {
  deck,
  errors,
  /** The zoom the drawn lines are for, or null before the first result. */
  zoomShown: (): number | null =>
    current()?.state?.result ? (current()?.state?.zoom ?? null) : null,
  /** Lines drawn now. */
  lines: (): number => current()?.state?.result?.lines.features.length ?? 0,
  /** Replaces the routes (the e2e test sends a bad one to see the last good result stay). */
  setRoutes(next: RoutesInput): void {
    routes = next;
    deck.setProps({ layers: [layer()] });
  },
  setCap(next: number | null): void {
    cap = next;
    deck.setProps({ layers: [layer()] });
  },
  dropped: () => droppedReports.map((ids) => ids.slice()),
  drawnIds: () => current()?.state?.result?.lines.features.map((f) => f.id) ?? [],
  /** Zooms the view to `zoom`, centered as at the start. */
  zoomTo(zoom: number): void {
    deck.setProps({ initialViewState: { ...INITIAL, zoom } });
  },
  picked: () => picked,
  screenOf(id, t) {
    const line = current()?.state?.result?.lines.features.find((f) => f.id === id);
    const [a, b] = line?.geometry.coordinates ?? [];
    const [viewport] = deck.getViewports();
    if (!a || !b || !viewport) return null;
    const at = (i: number): number => (a[i] ?? 0) + t * ((b[i] ?? 0) - (a[i] ?? 0));
    const [x = 0, y = 0] = viewport.project([at(0), at(1)]);
    return { x, y };
  },
};
declare global {
  interface Window {
    deckDebug?: DeckDebug;
  }
}
window.deckDebug = debug;
if (status) status.textContent = 'BraidLayer: three routes on one street (pan and zoom).';
