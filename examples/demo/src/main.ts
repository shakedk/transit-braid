// Demo: transit routes drawn side by side by `sideBySide()`, the library's main call, on a map.
//   - "Network": three made-up routes to edit, or 25 real bus routes in Tel Aviv with their stops
//     (loaded only when picked).
//   - "Side by side" off shows the routes as given, on top of each other (before).
//   - "Zoom" and "Lane cap" (`maxSideBySide`) draw again; a route keeps its lane at every zoom.
//   - The route list removes a route (✕) or picks one to move: drag its points on the map.
//   - "Add route": click points on the map, then "Finish" (at least two different points).
// Every change goes through one session (`createSession()`), so an edit redraws only what changed.
// deck.gl draws the lines over a MapLibre map. Add `?basemap=none` for a plain background (used
// by the e2e tests).
import 'maplibre-gl/dist/maplibre-gl.css';

import type { Layer } from '@deck.gl/core';
import { PathLayer, ScatterplotLayer, TextLayer } from '@deck.gl/layers';
import { MapboxOverlay } from '@deck.gl/mapbox';
import {
  Map as MapLibreMap,
  type MapMouseEvent,
  Marker,
  NavigationControl,
  ScaleControl,
  setWorkerUrl,
  type StyleSpecification,
} from 'maplibre-gl';
import maplibreWorkerUrl from 'maplibre-gl/dist/maplibre-gl-worker.mjs?url';
import { type BraidSession, createSession, type StationsInput } from 'transit-braid';

import { type Mark, marksAlong, metersPerPixel } from './chevrons.ts';
import {
  addRoute,
  type DemoRoute,
  movePoint,
  probeOf,
  removeRoute,
  type Rgb,
  toRoutesInput,
} from './edit.ts';
import { type Network, NETWORKS } from './networks.ts';
import type { LngLat } from './project.ts';

const LINE_WIDTH_PX = 6;
const SPACING_PX = 2;
/** Screen distance between two direction marks on a line. */
const MARK_EVERY_PX = 140;

// The plain background of `?basemap=none` (no map tiles: stable tests). Its one GeoJSON source,
// empty, makes MapLibre start its worker as a real map does, so the tests fail when the worker
// file is missing (it was, on the built site, until main.ts set its URL).
const BLANK_STYLE: StyleSpecification = {
  version: 8,
  sources: { empty: { type: 'geojson', data: { type: 'FeatureCollection', features: [] } } },
  layers: [
    { id: 'background', type: 'background', paint: { 'background-color': '#f4f1ea' } },
    { id: 'empty', type: 'line', source: 'empty' },
  ],
};

type Drawn = { lineId: string; path: LngLat[]; color: Rgb };

const params = new URLSearchParams(location.search);
/** `?basemap=none`: the plain background for every network (stable tests, no map tiles). */
const alwaysPlain = params.get('basemap') === 'none';
const STREETS_STYLE = 'https://tiles.openfreemap.org/styles/positron';
const styleFor = (n: Network): StyleSpecification | string =>
  alwaysPlain || n.basemap === 'plain' ? BLANK_STYLE : STREETS_STYLE;

// MapLibre builds its worker file's name at run time, so no bundler sees it: point MapLibre at the
// copy Vite emits for this import, or a built site has no worker and draws no map.
setWorkerUrl(maplibreWorkerUrl);

function byId<T extends HTMLElement>(id: string, kind: new () => T): T {
  const el = document.getElementById(id);
  if (!(el instanceof kind)) throw new Error(`#${id} is missing from the page`);
  return el;
}
const statusEl = byId('status', HTMLParagraphElement);
const chipEl = byId('map-chip', HTMLDivElement);
const networkSelect = byId('network', HTMLSelectElement);
const aboutEl = byId('network-about', HTMLParagraphElement);
const lookEl = byId('network-look', HTMLParagraphElement);
const creditEl = byId('network-credit', HTMLParagraphElement);
const sideBox = byId('side-by-side', HTMLInputElement);
const zoomInput = byId('zoom', HTMLInputElement);
const zoomOut = byId('zoom-value', HTMLOutputElement);
const capSelect = byId('cap', HTMLSelectElement);
const capAbout = byId('cap-about', HTMLParagraphElement);
const countEl = byId('route-count', HTMLSpanElement);
const listEl = byId('routes', HTMLUListElement);
const addButton = byId('add', HTMLButtonElement);
const finishButton = byId('finish', HTMLButtonElement);
const cancelButton = byId('cancel', HTMLButtonElement);

const [firstNetwork] = NETWORKS;
if (!firstNetwork) throw new Error('The demo has no networks');
/** `?network=<id>` opens that network (the Examples page links to each one this way). */
let network: Network = NETWORKS.find((n) => n.id === params.get('network')) ?? firstNetwork;
let shownStyle = styleFor(network);
const map = new MapLibreMap({
  container: 'map',
  style: shownStyle,
  center: network.center,
  zoom: network.zoom,
});
map.addControl(new NavigationControl(), 'top-right');
map.addControl(new ScaleControl({ unit: 'metric' }), 'bottom-left');

// interleaved: false = deck.gl draws on its own canvas above the map. Interleaved mode
// (lines under map labels) breaks with MapLibre 6 in deck.gl 9.4: it reads the removed
// `map.transform`.
const overlay = new MapboxOverlay({ interleaved: false, layers: [] });
map.addControl(overlay);

let routes: DemoRoute[] = [];
let stations: StationsInput | null = null;
/** The page's session: every edit and zoom starts from the last draw, so a zoom redraws only
 * the geometry and an edit keeps the lanes the routes had. A new network starts a new one. */
let session: BraidSession = createSession();
/** At most this many lines per street (`maxSideBySide`), or null for no cap. */
let cap: number | null = null;

/** What is drawn now (the last good result stays when a call fails). */
let drawn: Drawn[] = [];
/** Station bars, as paths across their bundles. */
let bars: LngLat[][] = [];
/** Direction marks along the drawn lines. */
let marks: Mark[] = [];
/** Routes the lane cap left out of the last draw. */
let dropped: string[] = [];
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

const toPath = (coordinates: readonly (readonly number[])[]): LngLat[] =>
  coordinates.map(([x = 0, y = 0]): LngLat => [x, y]);

/** Direction marks for what is drawn, spaced for the map's zoom. */
function placeMarks(): void {
  const spacing = MARK_EVERY_PX * metersPerPixel(map.getZoom(), map.getCenter().lat);
  marks = drawn.flatMap((d) => marksAlong(d.path, spacing));
}

function updateChip(): void {
  const zoom = map.getZoom();
  const mpp = metersPerPixel(zoom, map.getCenter().lat);
  const mode = sideBox.checked ? 'Side by side' : 'As given';
  chipEl.textContent = `${mode} · zoom ${zoom.toFixed(1)} · ${mpp.toFixed(2)} m/px`;
}

function render(): void {
  const layers: Layer[] = [
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
    new TextLayer<Mark>({
      id: 'direction-marks',
      data: marks,
      getPosition: (m) => m.position,
      getText: () => '>',
      getAngle: (m) => m.angle,
      getSize: 9,
      sizeUnits: 'pixels',
      getColor: [255, 255, 255, 235],
      fontWeight: 700,
      characterSet: ['>'],
      getTextAnchor: 'middle',
      getAlignmentBaseline: 'center',
    }),
    new PathLayer<LngLat[]>({
      id: 'station-bar-edges',
      data: bars,
      getPath: (b) => b,
      getColor: [29, 43, 47],
      getWidth: 7,
      widthUnits: 'pixels',
      capRounded: true,
    }),
    new PathLayer<LngLat[]>({
      id: 'station-bars',
      data: bars,
      getPath: (b) => b,
      getColor: [255, 255, 255],
      getWidth: 4,
      widthUnits: 'pixels',
      capRounded: true,
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
      new ScatterplotLayer<LngLat>({
        id: 'draft-points',
        data: draft,
        getPosition: (p) => p,
        getFillColor: [60, 60, 60],
        getRadius: 4,
        radiusUnits: 'pixels',
      }),
    );
  }
  overlay.setProps({ layers });
  updateChip();
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
    network: network.id,
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
    bars: bars.length,
    marks: marks.length,
    dropped,
  };
}

function updateCapAbout(): void {
  if (cap === null) {
    capAbout.textContent = 'No cap: every route is drawn.';
    return;
  }
  const left = dropped.length === 1 ? '1 route left out' : `${dropped.length} routes left out`;
  capAbout.textContent = `At most ${cap} per street: ${left} (whole routes, in input order).`;
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
    bars = [];
    dropped = [];
    shown = run;
    placeMarks();
    setStatus(`${routes.length} routes as given: they hide each other where they share a street.`);
    updateCapAbout();
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
        ...(stations === null ? {} : { stations }),
        ...(cap === null ? {} : { maxSideBySide: cap }),
      });
      if (current !== runs) continue; // a newer change came in: drop this answer
      const color = new Map(routes.map((r) => [r.id, r.color]));
      drawn = out.lines.features.map((f) => ({
        lineId: String(f.id),
        path: toPath(f.geometry.coordinates),
        color: color.get(String(f.id)) ?? [0, 0, 0],
      }));
      bars = out.stations.features.map((f) => toPath(f.geometry.coordinates));
      dropped = out.dropped.map(String);
      shown = current;
      placeMarks();
      const ms = performance.now() - started;
      setStatus(
        `${routes.length} routes side by side in ${ms.toFixed(1)} ms · zoom ${map.getZoom().toFixed(1)}`,
      );
      updateCapAbout();
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
  countEl.textContent = String(routes.length);
  listEl.replaceChildren(
    ...routes.map((r) => {
      const name = r.name ?? r.id;
      const li = document.createElement('li');
      li.dataset['route'] = r.id;
      const swatch = document.createElement('span');
      swatch.className = 'swatch';
      swatch.style.background = `rgb(${r.color.join(' ')})`;
      const pick = document.createElement('button');
      pick.type = 'button';
      pick.className = 'pick';
      pick.textContent = name;
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
      remove.setAttribute('aria-label', `Remove ${name}`);
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

// --- Networks ---

function showNetworkText(): void {
  aboutEl.textContent = network.about;
  lookEl.textContent = network.look === undefined ? '' : `What to look for: ${network.look}`;
  lookEl.hidden = network.look === undefined;
  creditEl.textContent = network.credit ?? '';
  creditEl.hidden = network.credit === undefined;
}

/** Shows `next`: its routes and stops, in a new session, centered where it is. */
async function pickNetwork(next: Network): Promise<void> {
  network = next;
  showNetworkText();
  setStatus(`Loading ${next.name}…`);
  editing = null;
  showMarkers();
  if (draft !== null) setDrafting(false);
  try {
    const loaded = await next.load();
    if (network !== next) return; // another network was picked meanwhile
    routes = loaded.routes;
    stations = loaded.stations;
  } catch (error) {
    setStatus(
      `Could not load ${next.name}: ${error instanceof Error ? error.message : String(error)}`,
    );
    return;
  }
  session.close();
  session = createSession();
  renderList();
  const nextStyle = styleFor(next);
  if (nextStyle !== shownStyle) {
    shownStyle = nextStyle;
    map.setStyle(nextStyle);
  }
  if (next.bounds) map.fitBounds(next.bounds, { padding: 48, duration: 0 });
  else map.jumpTo({ center: next.center, zoom: next.zoom });
  const url = new URL(location.href);
  url.searchParams.set('network', next.id);
  history.replaceState(null, '', url);
  redraw();
}

/** A labeled group of the network picker's options. */
function optionGroup(label: string, networks: readonly Network[]): HTMLOptGroupElement {
  const group = document.createElement('optgroup');
  group.label = label;
  group.append(
    ...networks.map((n) => {
      const option = document.createElement('option');
      option.value = n.id;
      option.textContent = n.name;
      return option;
    }),
  );
  return group;
}
networkSelect.replaceChildren(
  optionGroup(
    'Real streets',
    NETWORKS.filter((n) => n.basemap !== 'plain'),
  ),
  optionGroup(
    'Scenarios (made up, plain background)',
    NETWORKS.filter((n) => n.basemap === 'plain'),
  ),
);
networkSelect.value = network.id;
networkSelect.addEventListener('change', () => {
  const next = NETWORKS.find((n) => n.id === networkSelect.value);
  if (next) void pickNetwork(next);
});

// --- Drawing, zoom and lane cap ---

sideBox.addEventListener('change', redraw);
capSelect.addEventListener('change', () => {
  cap = capSelect.value === '' ? null : Number(capSelect.value);
  redraw();
});
zoomInput.addEventListener('input', () => {
  map.jumpTo({ zoom: Number(zoomInput.value) });
});
map.on('zoom', () => {
  const zoom = map.getZoom();
  zoomInput.value = String(zoom);
  zoomOut.textContent = zoom.toFixed(1);
  updateChip();
});

map.on('load', () => {
  addButton.disabled = false;
  void pickNetwork(network);
});
// Widths are in pixels, so the offsets in meters change with zoom: draw again after zooming.
map.on('zoomend', () => {
  if (sideBox.checked) redraw();
  else {
    placeMarks();
    render();
  }
});
zoomInput.value = String(map.getZoom());
zoomOut.textContent = map.getZoom().toFixed(1);
