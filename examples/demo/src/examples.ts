// The Examples page: every network the demo shows, as a card with a small picture of its routes
// (as given, before transit-braid), what it shows and what to look for. Each card opens the demo
// with that network (`./?network=<id>`).
import { type DemoRoute, routesFromGeoJson, SAMPLE_ROUTES } from './edit.ts';
import { type Network, NETWORKS } from './networks.ts';
import type { LngLat } from './project.ts';
import { SCENARIOS } from './scenarios.ts';
import { fitPaths } from './thumbnail.ts';

const SVG = 'http://www.w3.org/2000/svg';
const WIDTH = 280;
const HEIGHT = 150;

/** A picture of `routes`, each in its color, with a dot per stop. */
function picture(routes: readonly DemoRoute[], stops: readonly LngLat[]): SVGSVGElement {
  const svg = document.createElementNS(SVG, 'svg');
  svg.setAttribute('viewBox', `0 0 ${WIDTH} ${HEIGHT}`);
  svg.setAttribute('aria-hidden', 'true');
  svg.classList.add('thumb');
  const fitted = fitPaths([...routes.map((r) => r.points), stops], WIDTH, HEIGHT, 14);
  for (const [i, route] of routes.entries()) {
    const line = document.createElementNS(SVG, 'polyline');
    line.setAttribute(
      'points',
      (fitted[i] ?? []).map(([x, y]) => `${x.toFixed(1)},${y.toFixed(1)}`).join(' '),
    );
    line.setAttribute('stroke', `rgb(${route.color.join(' ')})`);
    svg.append(line);
  }
  for (const [x, y] of fitted.at(-1) ?? []) {
    const dot = document.createElementNS(SVG, 'circle');
    dot.setAttribute('cx', x.toFixed(1));
    dot.setAttribute('cy', y.toFixed(1));
    dot.setAttribute('r', '2.5');
    svg.append(dot);
  }
  return svg;
}

/** A card linking to the demo with `network`. */
function card(network: Network, thumb: SVGSVGElement | null): HTMLAnchorElement {
  const a = document.createElement('a');
  a.className = 'card';
  a.href = `./?network=${encodeURIComponent(network.id)}`;
  a.dataset['network'] = network.id;
  const title = document.createElement('h3');
  title.textContent = network.name;
  const about = document.createElement('p');
  about.textContent = network.about;
  a.append(...(thumb ? [thumb] : []), title, about);
  if (network.look) {
    const look = document.createElement('p');
    look.className = 'look';
    look.textContent = `What to look for: ${network.look}`;
    a.append(look);
  }
  const open = document.createElement('span');
  open.className = 'open';
  open.textContent = 'Open in the demo →';
  a.append(open);
  return a;
}

const stopsOf = (id: string): LngLat[] =>
  (SCENARIOS.find((s) => s.id === id)?.stations?.features ?? []).map((f): LngLat => [
    f.geometry.coordinates[0] ?? 0,
    f.geometry.coordinates[1] ?? 0,
  ]);

const realList = document.getElementById('real');
const scenarioList = document.getElementById('scenarios');
if (!realList || !scenarioList) throw new Error('The Examples page is missing its lists');

for (const network of NETWORKS) {
  if (network.basemap === 'plain') {
    const scenario = SCENARIOS.find((s) => s.id === network.id);
    scenarioList.append(
      card(network, scenario ? picture(scenario.routes, stopsOf(network.id)) : null),
    );
  } else if (network.id === 'sample') {
    realList.append(card(network, picture(SAMPLE_ROUTES, [])));
  } else {
    // Real data loads on its own; the card shows first, its picture once the routes arrive.
    const item = card(network, null);
    realList.append(item);
    if (network.id === 'tel-aviv') {
      void fetch('data/tel-aviv-routes.geojson')
        .then(async (r) => (r.ok ? routesFromGeoJson(await r.json()) : []))
        .then((routes) => {
          if (routes.length > 0) item.prepend(picture(routes, []));
        })
        .catch(() => undefined);
    }
  }
}
document.body.dataset['ready'] = 'true';
