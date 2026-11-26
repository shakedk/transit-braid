// Demo: a few routes drawn side by side by `sideBySide()`, the library's main call, and
// edited on the map. Every change (add, remove or move a route, zoom) calls `sideBySide()` again:
//   - "Side by side" off shows the routes as given, on top of each other (before).
//   - The list removes a route (✕) or picks one to move: drag its points on the map.
//   - "Add route": click points on the map, then "Finish" (at least two different points).
// deck.gl draws the lines over a MapLibre map. Add `?basemap=none` for a plain background (used
// by the e2e tests).
import 'maplibre-gl/dist/maplibre-gl.css';

import { PathLayer, ScatterplotLayer } from '@deck.gl/layers';
import { MapboxOverlay } from '@deck.gl/mapbox';
import {
  Map as MapLibreMap,
  type MapMouseEvent,
  Marker,
  NavigationControl,
  type StyleSpecification,
} from 'maplibre-gl';
import { createSession } from 'transit-braid';

import {
  addRoute,
  type DemoRoute,
  movePoint,
  probeOf,
  removeRoute,
  type Rgb,
  SAMPLE_ROUTES,
  toRoutesInput,
} from './edit.ts';
import type { LngLat } from './project.ts';

const LINE_WIDTH_PX = 6;
const SPACING_PX = 2;

const BLANK_STYLE: StyleSpecification = {
  version: 8,
  sources: {},
  layers: [{ id: 'background', type: 'background', paint: { 'background-color': '#f4f1ea' } }],
};

type Drawn = { lineId: string; path: LngLat[]; color: Rgb };

const params = new URLSearchParams(location.search);
const style =
  params.get('basemap') === 'none' ? BLANK_STYLE : 'https://tiles.openfreemap.org/styles/liberty';

const map = new MapLibreMap({ container: 'map', style, center: [34.7775, 32.0765], zoom: 14.5 });
map.addControl(new NavigationControl(), 'top-right');

// interleaved: false = deck.gl draws on its own canvas above the map. Interleaved mode
// (lines under map labels) breaks with MapLibre 6 in deck.gl 9.4: it reads the removed
// `map.transform`.
const overlay = new MapboxOverlay({ interleaved: false, layers: [] });
map.addControl(overlay);

function byId<T extends HTMLElement>(id: string, kind: new () => T): T {
  const el = document.getElementById(id);
  if (!(el instanceof kind)) throw new Error(`#${id} is missing from the page`);
  return el;
}
const statusEl = byId('status', HTMLSpanElement);
const sideBox = byId('side-by-side', HTMLInputElement);
const listEl = byId('routes', HTMLUListElement);
const addButton = byId('add', HTMLButtonElement);
const finishButton = byId('finish', HTMLButtonElement);
const cancelButton = byId('cancel', HTMLButtonElement);

let routes: DemoRoute[] = structuredClone([...SAMPLE_ROUTES]);
/** The page's session: every edit and zoom starts from the last draw, so a zoom redraws only
 * the geometry and an edit keeps the lanes the routes had. */
const session = createSession();

/** What is drawn now (the last good result stays when a call fails). */
let drawn: Drawn[] = [];
/** The route whose points can be dragged, or null. */
let editing: string | null = null;
/** Points of the route being added, or null when not adding. */
let draft: LngLat[] | null = null;
/** Changes so far: each edit, toggle or zoom asks for a draw. */
let runs = 0;
/**
 * `sideBySide()` calls made. One runs at a time; changes that arrive meanwhile wait, and only
 * the newest of them runs next (the older ones are never computed), so rapid edits cost at most
 * one stale call.
 */
let calls = 0;
/** The change whose result is on screen. */
let shown = 0;
/** A `sideBySide()` call is under way. */
let busy = false;
let status = '';

const setStatus = (text: string): void => {
  status = text;
  statusEl.textContent = text;
};

function render(): void {
  const layers = [
    new PathLayer<Drawn>({
      id: 'braid-lines',
      data: drawn,
      getPath: (d) => d.path,
      getColor: (d) => d.color,
      getWidth: LINE_WIDTH_PX,
      widthUnits: 'pixels',
      capRounded: false,
      jointRounded: false,
      miterLimit: 4,
      pickable: true,
    }),
  ];
  if (draft !== null) {
    layers.push(
      new PathLayer<Drawn>({
        id: 'draft-line',
        data: [{ lineId: 'draft', path: draft, color: [60, 60, 60] }],
        getPath: (d) => d.path,
        getColor: (d) => d.color,
        getWidth: 2,
        widthUnits: 'pixels',
      }),
    );
  }
  const dots = draft === null ? [] : draft;
  overlay.setProps({
    layers: [
      ...layers,
      new ScatterplotLayer<LngLat>({
        id: 'draft-points',
        data: dots,
        getPosition: (p) => p,
        getFillColor: [60, 60, 60],
        getRadius: 4,
        radiusUnits: 'pixels',
      }),
    ],
  });
  publish();
}

function publish(): void {
  const probes: Record<string, LngLat> = {};
  for (const d of drawn) {
    const p = probeOf(d.path);
    if (p) probes[d.lineId] = p;
  }
  window.braidDebug = {
    map,
    overlay,
    ready: true,
    sideBySide: sideBox.checked,
    routes: routes.map((r) => r.id),
    runs,
    calls,
    shown,
    move: (routeId: string, index: number, to: LngLat) => {
      routes = movePoint(routes, routeId, index, to);
      redraw();
    },
    editing,
    drafting: draft?.length ?? null,
    status,
    probes,
  };
}

/**
 * Draws the routes: as given, or side by side through `sideBySide()` at the map's zoom. While a
 * call runs, new changes only bump `runs`; when it ends, an answer for an older change is
 * dropped and the newest change runs instead. A failed call keeps the last good result on screen.
 */
async function draw(): Promise<void> {
  const run = ++runs;
  if (!sideBox.checked) {
    drawn = routes.map((r) => ({ lineId: r.id, path: r.points, color: r.color }));
    shown = run;
    setStatus(`${routes.length} routes as given: they hide each other where they share a street.`);
    render();
    return;
  }
  if (busy) return; // the call under way runs the newest change when it ends
  busy = true;
  let handled = 0;
  while (handled !== runs && sideBox.checked) {
    const current = runs;
    handled = current;
    const started = performance.now();
    calls += 1;
    try {
      // oxlint-disable-next-line eslint/no-await-in-loop -- one call at a time is the point
      const out = await session.sideBySide(toRoutesInput(routes), {
        zoom: map.getZoom(),
        lineWidth: LINE_WIDTH_PX,
        spacing: SPACING_PX,
      });
      if (current !== runs) continue; // a newer change came in: drop this answer
      const color = new Map(routes.map((r) => [r.id, r.color]));
      drawn = out.lines.features.map((f) => ({
        lineId: String(f.id),
        path: f.geometry.coordinates.map(([x = 0, y = 0]): LngLat => [x, y]),
        color: color.get(String(f.id)) ?? [0, 0, 0],
      }));
      shown = current;
      const ms = performance.now() - started;
      setStatus(
        `${routes.length} routes side by side in ${ms.toFixed(1)} ms · zoom ${map.getZoom().toFixed(1)}`,
      );
    } catch (error) {
      if (current !== runs) continue;
      setStatus(`Error: ${error instanceof Error ? error.message : String(error)}`);
    }
  }
  busy = false;
  render();
}

const redraw = (): void => {
  void draw();
};

// --- Moving a route: draggable markers on its points ---

let markers: Marker[] = [];

function showMarkers(): void {
  for (const m of markers) m.remove();
  markers = [];
  const route = routes.find((r) => r.id === editing);
  if (!route) return;
  markers = route.points.map((p, index) => {
    const marker = new Marker({ draggable: true, scale: 0.6 }).setLngLat(p).addTo(map);
    marker.getElement().dataset['point'] = String(index);
    marker.on('dragend', () => {
      const { lng, lat } = marker.getLngLat();
      routes = movePoint(routes, route.id, index, [lng, lat]);
      redraw();
    });
    return marker;
  });
}

// --- The route list ---

function renderList(): void {
  listEl.replaceChildren(
    ...routes.map((r) => {
      const li = document.createElement('li');
      li.dataset['route'] = r.id;
      const swatch = document.createElement('span');
      swatch.className = 'swatch';
      swatch.style.background = `rgb(${r.color.join(' ')})`;
      const pick = document.createElement('button');
      pick.type = 'button';
      pick.className = 'pick';
      pick.textContent = r.id;
      pick.setAttribute('aria-pressed', String(editing === r.id));
      pick.title = 'Move this route: drag its points on the map';
      pick.addEventListener('click', () => {
        editing = editing === r.id ? null : r.id;
        renderList();
        showMarkers();
        publish();
      });
      const remove = document.createElement('button');
      remove.type = 'button';
      remove.className = 'remove';
      remove.textContent = '✕';
      remove.setAttribute('aria-label', `Remove ${r.id}`);
      remove.addEventListener('click', () => {
        routes = removeRoute(routes, r.id);
        if (editing === r.id) editing = null;
        renderList();
        showMarkers();
        redraw();
      });
      li.append(swatch, pick, remove);
      return li;
    }),
  );
}

// --- Adding a route ---

function setDrafting(on: boolean): void {
  draft = on ? [] : null;
  addButton.hidden = on;
  finishButton.hidden = !on;
  cancelButton.hidden = !on;
  map.getCanvas().style.cursor = on ? 'crosshair' : '';
  render();
}

map.on('click', (e: MapMouseEvent) => {
  if (draft === null) return;
  draft = [...draft, [e.lngLat.lng, e.lngLat.lat]];
  render();
});
addButton.addEventListener('click', () => {
  setDrafting(true);
  setStatus('Click points on the map, then “Finish”.');
});
cancelButton.addEventListener('click', () => {
  setDrafting(false);
  redraw();
});
finishButton.addEventListener('click', () => {
  const next = addRoute(routes, draft ?? []);
  if (typeof next === 'string') {
    setStatus(next);
    publish();
    return;
  }
  routes = next;
  setDrafting(false);
  renderList();
  redraw();
});

sideBox.addEventListener('change', redraw);

map.on('load', () => {
  renderList();
  addButton.disabled = false;
  redraw();
});
// Widths are in pixels, so the offsets in meters change with zoom: draw again after zooming.
map.on('zoomend', () => {
  if (sideBox.checked) redraw();
});
