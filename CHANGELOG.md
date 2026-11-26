# Changelog

All notable changes to the npm packages `transit-braid` and `transit-braid-deck` are listed here.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the versions
follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html): while the version is 0.x, a
minor version (0.2.0) may change the API.

## [Unreleased]

## [0.1.2] - 2026-11-26

### Changed

- The README on npm shows the logo and the picture of how stops get their bars. No change to
  the code.

## [0.1.1] - 2026-11-26

### Changed

- The README on npm shows a picture of what transit-braid draws, and links to the live demo and
  the repository. No change to the code.

## [0.1.0] - 2026-11-26

The first version.

### Added

**What it does.** `transit-braid` takes bus, tram or rail routes that overlap, as GeoJSON LineStrings,
and returns GeoJSON in which each route has its own lane wherever routes share a street, so they
run side by side instead of on top of each other, plus a station bar across each bundle. The work
runs in Rust compiled to WebAssembly, off the page's main thread, and the result is plain
geometry, so any map library can draw it. Routes are never merged: each input route comes out as
one line, with its id, direction and properties. The order of the lanes is the same at every zoom.

**The call.** `sideBySide(routes, options)` resolves to `{ lines, stations, dropped }`.

- `routes`: a GeoJSON FeatureCollection of LineStrings in `[lng, lat]`, each with its own GeoJSON
  `id` (a string or a number).
- The map scale, exactly one of: `zoom`, 0 to 24, in 512-pixel tiles (the zoom of MapLibre GL,
  Mapbox GL and deck.gl; for 256-pixel tiles, as in Leaflet, pass the zoom − 1), or
  `metersPerPixel`, ground meters per screen pixel where the routes are.
- `lineWidth` and `spacing`, in pixels (default 4 and 1: `DEFAULT_LINE_WIDTH`, `DEFAULT_SPACING`).
- `stations`: a GeoJSON FeatureCollection of Points. Each station near a route gets a bar across
  the whole bundle, with the station's id and properties; stations with the same
  `properties.name` within 50 m of each other are drawn as one bar.
- `maxSideBySide`: a lane cap. Routes are kept in the order given while every street they use has
  room; the others are left out whole and listed in `dropped`, the same at every zoom.

`sideBySide()` is pure: the same input always gives the same output, whatever ran before. A zoom
change is still fast: with exactly the last call's routes, stations and lane cap, only the
geometry is drawn again, with the same result a first call gives. That is the only reuse, so it
stays pure: an edit draws from scratch here. Fast edits that keep the other routes' lanes are a
session's job (below).

**Sessions.** `createSession()` returns a `BraidSession`, `{ sideBySide, close }`, for an editor
or a map that redraws: it remembers its last call, so a zoom change redraws only the geometry (a
few milliseconds, more with many stations) and an edit rebuilds only what changed and keeps the
lanes the other routes had.
Its calls run in the order made. `close()` frees its memory; later calls reject with `CLOSED`.

**Workers.** `sideBySide()` and `createSession()` share one worker, started on the first call.
`createBraidWorker()` starts a `BraidWorker` of your own (`sideBySide`, `createSession`,
`terminate`); after `terminate()`, calls reject with `TERMINATED`.

**Three entries,** with the same names and types:

- `transit-braid`: in a Web Worker; the bundler copies the worker script and the `.wasm` next to
  the app (`new URL(..., import.meta.url)`, as Vite and webpack 5 do). Under Node it resolves to
  the Node entry.
- `transit-braid/inline`: the same from one file, with the worker and the WebAssembly inside, for
  bundlers that can't copy those files.
- `transit-braid/node`: Node 24+, on the caller's thread; `createBraidWorker()` runs the work on
  a `worker_threads` thread instead.

**Errors.** Every error is a `BraidError` with a stable `code`:

- `InputError`: the input can't be drawn. Codes: `BAD_OPTION`, `BAD_ROUTES`, `MISSING_ID`,
  `DUPLICATE_ID`, `TOO_MANY_ROUTES`, `BAD_COORDINATE`, `TOO_FEW_POINTS`, `TOO_MANY_POINTS`,
  `CROSSES_ANTIMERIDIAN`, `TOO_WIDE`, `TOO_LONG`, `TOO_COMPLEX`, `BAD_STATION`,
  `TOO_MANY_STATIONS`. Where they apply, `routeId`, `index`, `point`, `stationId` and `option`
  say where.
- `InternalError`: transit-braid itself failed, never the input's fault (`ENGINE_CRASHED`,
  `WORKER_FAILED`, `INTERNAL`). It recovers by itself: the next call works.
- A plain `BraidError` for a call on a closed session (`CLOSED`) or a terminated worker
  (`TERMINATED`).

`instanceof` works across copies of the package (for example the inline entry next to the main
one).

**Limits,** each refused with its own code before any heavy work: at most 70 routes; 1,000,000
points in all; routes that spread at most 200 km east-west and north-south and are at most
2,000 km long together; no route crossing the 180° meridian; at most 5,000 stations. A work budget
stops input that is valid but too much work to draw (`TOO_COMPLEX`) as soon as it runs out.

**`transit-braid-deck`:** `BraidLayer`, a deck.gl 9 layer that draws the routes with
`transit-braid`. Its `data` is what `sideBySide()` takes; `stations`, `lineWidth`, `spacing`,
`maxSideBySide` and `onDropped` (the left-out ids) as there; `getColor` (default: the route's
`properties.color`, else gray: `colorOf`, `DEFAULT_COLOR`). It keeps a session while it is on the
map, draws again when its props change and once the zoom has stayed still for 150 ms
(`ZOOM_SETTLE_MS`), keeps the last good lines on screen meanwhile, and sends a failed call to
deck.gl's `onError`. Picking a line gives its route. `transit-braid` and deck.gl are peer
dependencies.

### Known limits

- Speed: an edit in a session takes about 90 to 140 ms on 50-route city networks in Chromium
  (up to about 200 ms at 70 routes, the limit), and the first draw of a big city up to about a
  second. Plain `sideBySide()` draws everything
  again for any change but the scale, line width and spacing: an editor uses a session. A zoom
  change redraws the station bars too: about 100 ms with 1,875 stations on a 50-route network.
- Where junctions are very close together, and at some roundabouts, lines can take small jogs.

[Unreleased]: https://github.com/shakedk/transit-braid/compare/v0.1.2...HEAD
[0.1.2]: https://github.com/shakedk/transit-braid/releases/tag/v0.1.2
[0.1.1]: https://github.com/shakedk/transit-braid/releases/tag/v0.1.1
[0.1.0]: https://github.com/shakedk/transit-braid/releases/tag/v0.1.0
