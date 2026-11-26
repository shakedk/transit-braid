# transit-braid

Side-by-side transit lines for web maps. Give it overlapping bus, tram or rail routes as GeoJSON
LineStrings; it returns GeoJSON with each route in its own lane where routes share a street, plus
station bars across each bundle. The work runs in Rust compiled to WebAssembly, off the page's
main thread, and the result is geometry only, so any map library can draw it.

> **Status:** early development (0.x). The API may still change between minor versions.

## Install

```sh
npm install transit-braid
```

Every current browser (Web Workers and WebAssembly with SIMD), or Node 24+. No runtime
dependencies.

## Use

```ts
import { sideBySide } from 'transit-braid';

const { lines, stations, dropped } = await sideBySide(routes, { zoom: 13 });
```

- `routes`: a GeoJSON FeatureCollection of up to 70 LineStrings in `[lng, lat]`, each with its
  own GeoJSON `id`.
- `lines`: one LineString per route, in input order, with the same id, direction and properties.
- The scale is exactly one of `zoom` or `metersPerPixel`. `zoom` counts 512-pixel tiles, as
  MapLibre GL, Mapbox GL and deck.gl do; for a 256-pixel-tile map (Leaflet, OpenLayers) pass its
  zoom − 1, or give `metersPerPixel` (ground meters per screen pixel where the routes are).
- Optional: `lineWidth` and `spacing` in pixels (default 4 and 1); `stations`, a FeatureCollection
  of Points, each near a route getting a bar in `stations`; `maxSideBySide`, a lane cap whose
  left-out routes are listed in `dropped`.

`sideBySide()` is pure: the same input always draws the same way, and a zoom change of the same
routes and stations redraws only the geometry. An editor uses a session instead: it remembers its
last call, so an edit rebuilds only what changed and keeps the lanes the routes had.

```ts
import { createSession } from 'transit-braid';

const session = createSession();
const out = await session.sideBySide(routes, { zoom: map.getZoom() });
// … again on every edit or zoom; when done:
session.close();
```

## Errors

Bad input rejects with an `InputError` whose `code` says why (for example `BAD_COORDINATE`,
`DUPLICATE_ID`, `TOO_MANY_ROUTES`) and whose `routeId`, `index`, `point`, `stationId` or `option`
says where. A failure inside transit-braid rejects with an `InternalError` and recovers by itself.
A call on a closed session or a terminated worker rejects with a `BraidError` (`CLOSED`,
`TERMINATED`). All three are `BraidError`s, and `instanceof` works across copies of the package.

## Entries

| Import                 | Runs                                                                                                                                                                                                                  |
| ---------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `transit-braid`        | In a Web Worker. Your bundler copies the worker script and the `.wasm` next to your code: one that follows `new URL(..., import.meta.url)`, such as Vite or webpack 5. Under Node it is `transit-braid/node`.         |
| `transit-braid/inline` | The same, from one file with the worker and the WebAssembly inside: for a page without a bundler (copy `dist/inline.js` next to it and import it from a `<script type="module">`), or a bundler that can't copy them. |
| `transit-braid/node`   | In Node 24+, on the caller's thread. `createBraidWorker()` runs the work on a `worker_threads` thread instead.                                                                                                        |

All three export the same names. A deck.gl layer is in
[`transit-braid-deck`](https://www.npmjs.com/package/transit-braid-deck).

## License

[MIT](LICENSE). More in the [repository](https://github.com/shakedk/transit-braid).
