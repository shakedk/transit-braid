# Development guide

How to set up the project, run it, and test each part: the full reference, for people and AI
agents working on the code. New here? [CONTRIBUTING.md](CONTRIBUTING.md) is the short version.
Keep this file up to date: every PR that adds a part, command or tool also updates this guide.

- [1. One-time setup](#1-one-time-setup)
- [2. Everyday commands](#2-everyday-commands)
- [3. Testing each part](#3-testing-each-part): how we test, then each part (Rust core,
  TypeScript package, WebAssembly bridge, worker, inline and Node entries, packaging, demo,
  deck.gl layer, test kit, golden harness)
- [4. Project layout](#4-project-layout)
- [5. Commit hooks and CI](#5-commit-hooks-and-ci)
- [6. Workflow: branches, commits, PRs](#6-workflow-branches-commits-prs)
- [7. Releasing](#7-releasing)
- [8. Troubleshooting](#8-troubleshooting)

## 1. One-time setup

You need these tools. Versions are pinned in the repo, so newer installers are fine.

| Tool    | Why                        | Install (macOS, no sudo)                                                                       |
| ------- | -------------------------- | ---------------------------------------------------------------------------------------------- |
| git     | Version control            | `xcode-select --install`                                                                       |
| Node 24 | Runs the TypeScript tools  | [nvm](https://github.com/nvm-sh/nvm): `nvm install 24`                                         |
| pnpm 12 | Installs JS packages       | `corepack enable pnpm` (comes with Node)                                                       |
| Rust    | Builds the core algorithms | [rustup](https://rustup.rs): `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh` |

You don't pick a Rust version: `rust-toolchain.toml` pins **Rust 1.99.1** (plus rustfmt, clippy
and the `wasm32-unknown-unknown` target) and rustup installs it on the first `cargo` command.

`pnpm setup:tools` (below) puts its tools in `~/.local/bin`, so that folder must be on your
`PATH`, or `pnpm check` can't find them. If it isn't, run this once and open a new terminal:

```sh
echo 'export PATH="$HOME/.local/bin:$PATH"' >> ~/.zshrc
```

Then:

```sh
git clone https://github.com/shakedk/transit-braid.git
cd transit-braid
pnpm install      # JS packages + commit hooks (lefthook)
pnpm setup:tools  # wasm-bindgen, wasm-opt, gitleaks, cargo-deny, zizmor: pinned, SHA-256 checked, into ~/.local/bin
pnpm exec playwright install chromium firefox webkit   # test browsers (once; Firefox and WebKit only run the smoke test)
pnpm check        # everything should pass
```

If `pnpm check` passes, you're set up.

## 2. Everyday commands

Run from the repo root.

| Command                     | What it does                                                                                                                             |
| --------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------- |
| `pnpm check`                | **Everything**: the workflow lint, all TypeScript checks, then all Rust checks                                                           |
| `pnpm check:ts`             | TS format check → build WASM and the packages → lint → type check → tests → e2e → pack test                                              |
| `pnpm check:rust`           | `cargo fmt --check` → `clippy -D warnings` (native + WASM) → `cargo test`, `--locked` (exactly what `Cargo.lock` says)                   |
| `pnpm build:wasm`           | Build Rust → WebAssembly into `packages/transit-braid/wasm/`, check size budget                                                          |
| `pnpm build:packages`       | Build both npm packages into their `dist/` (tsc, plus Vite for the one-file `transit-braid/inline`; needs `pnpm build:wasm`)             |
| `pnpm test:pack`            | Pack both packages, install the tarballs into a throwaway project, use them from Node, tsc and a Vite page in Chromium (needs the build) |
| `pnpm demo`                 | Start the demo map at http://localhost:5199                                                                                              |
| `pnpm e2e`                  | End-to-end tests: opens the demo in Chromium and checks the lines (Playwright)                                                           |
| `pnpm build:site`           | Build the demo as a static site with the inline entry (`examples/demo/dist-site/`; needs `pnpm build:packages`)                          |
| `pnpm e2e:site`             | The demo's, deck page's and Examples page's e2e tests on that site, as a static server serves it                                         |
| `pnpm setup:tools`          | Install/verify pinned wasm-bindgen, wasm-opt, gitleaks, cargo-deny, zizmor (`pnpm setup:tools gitleaks`: only the ones named)            |
| `pnpm secrets`              | Scan the whole git history for leaked secrets (gitleaks)                                                                                 |
| `pnpm lint:workflows`       | Lint the GitHub Actions workflows and `dependabot.yml` for security problems (zizmor, offline; skipped if zizmor isn't installed)        |
| `pnpm format`               | Auto-format TS, JSON, Markdown, YAML, TOML (oxfmt)                                                                                       |
| `cargo fmt --all`           | Auto-format Rust                                                                                                                         |
| `pnpm lint`                 | Lint TypeScript with type information (oxlint)                                                                                           |
| `pnpm typecheck`            | Type-check TypeScript (TypeScript 7 `tsc`)                                                                                               |
| `pnpm test`                 | Run TS tests once: Node + browsers                                                                                                       |
| `pnpm exec vitest`          | Run TypeScript tests in watch mode while you code                                                                                        |
| `cargo test --workspace`    | Run Rust tests                                                                                                                           |
| `cargo deny --locked check` | Check Rust dependency licenses and security advisories (the pinned cargo-deny)                                                           |
| `pnpm audit --prod`         | Known security advisories in the npm packages that ship (and what they depend on)                                                        |

## 3. Testing each part

**How we test, in short.** Every test runs the real code: no fakes or mocks, and bad input
first (empty, missing, malformed, too big). The kinds:

- Rust unit and property tests (about 280, `cargo test`): the algorithms, their limits and
  errors, and properties checked on thousands of random inputs.
- TypeScript tests in Node and in Chromium (278, Vitest): the API, the worker, crash recovery,
  every error code.
- End-to-end tests (15, Playwright in Chromium): the demo page and the deck.gl layer, as a user
  sees them. Firefox and WebKit run one smoke test each.
- The package test (`pnpm test:pack`): the packed npm tarballs installed in a fresh project
  and used from Node, TypeScript and a Vite page.
- The golden digests (below): every drawing pinned, so nothing changes by accident.
- The quality rules Q1–Q8 on every committed fixture (`quality.rs`), each known miss written
  down with its reason.

`pnpm check` runs all of them, in about two minutes on a laptop once the caches are warm.

Each part of the project, how to test it, and how to see it:

### Rust core — `crates/transit-braid-core`

The algorithms: geometry, corridor detection, line ordering.

```sh
cargo test -p transit-braid-core                 # unit + property tests
cargo test -p transit-braid-core offset          # only tests whose name contains "offset"
cargo clippy -p transit-braid-core --all-targets -- -D warnings
```

- **Unit tests** live next to the code, in a `#[cfg(test)] mod tests` block at the bottom of
  each file (e.g. `src/geom.rs`, `src/polyline.rs`); a module folder keeps them in sibling
  `*_tests.rs` files (e.g. `src/topology/split_tests.rs`).
- **Property tests** (`proptest!`) try hundreds of random inputs and check a rule that must
  always hold, e.g. "an offset segment is parallel and exactly the right distance away". If one
  fails, proptest prints the smallest input that breaks it.
- **Strict lints** (the workspace's `[workspace.lints]` in `Cargo.toml`) hold the
  production crates (core and wasm): no `unwrap`, `expect`, `panic!`, `todo!`, `unreachable!`,
  `dbg!` or printing; public functions document their errors and panics; every exception is
  `#[expect(lint, reason = "…")]`. `clippy.toml` bans platform math (`sin`, `cos`, `exp`, `ln`,
  `powi`, `atan2`… for `f64` and `f32`; `sqrt` is exact and allowed), plus `hypot` and `mul_add`
  (slow in WASM): use `det.rs`. Tests may unwrap and panic (`clippy.toml`) and use platform math
  (the core's `lib.rs`); the testkit has its own `[lints]` without the
  strict set. `pnpm check:rust` runs clippy with `-D warnings`, so any of these fails it.

### TypeScript core package — `packages/transit-braid`

The public API that apps import:

```ts
import { createSession, sideBySide } from 'transit-braid';
const { lines, stations } = await sideBySide(routes, { zoom: 14 }); // pure
await sideBySide(routes, { zoom: 15 }); // pure, and the same routes: geometry only

const session = createSession(); // an editor or a map that redraws
await session.sideBySide(routes, { zoom: 14 });
await session.sideBySide(routes, { zoom: 15 }); // geometry only
await session.sideBySide(edited, { zoom: 15 }); // incremental, lanes kept
session.close();
```

**Pure call and sessions.** `sideBySide()` is pure: the same input gives the same output
whatever ran before. For a fast zoom, each WASM instance (each worker, and Node's own thread) keeps
one engine cache for plain calls, built for the last plain call's input (`run.ts`): a call with
exactly those routes, stations and lane cap, compared to the bit, reruns geometry only at its own
scale, line width and spacing, which gives the bits a fresh cache gives; any other input frees that
cache and draws from an empty one, so a plain call never starts from an earlier call's order. A
crash drops it with the instance; a terminated worker frees it with its thread. A session (`createSession()`, or `worker.createSession()` on your own worker) keeps the
engine's cache between its own calls: a zoom change reruns geometry only, and an edit rebuilds the
graph incrementally and orders warm from the last order, so lines keep their lanes. Its calls run in
the order made. `close()` frees its memory (about 10 MB for a 50-route network); calls
after it reject with a `BraidError` coded `CLOSED`, while calls made before still finish. Each
session numbers its own route ids (`RouteKeys`). In the worker, `run.ts` keeps one `EngineCache`
(the WASM class holding the engine's saved work) per open session id, made on its first call; after a crash every session starts over empty. The
deck.gl layer and the demo page each draw through one session.
Tests: `src/session.test.ts` (Node: purity with the stress grid, whose warm edit differs from a
fresh one; the plain call's fast zoom: a zoom, scale, width or spacing change gives a fresh call's
bits and is much faster, other routes, stations or caps and edits draw what a fresh call draws, the
replaced cache is freed; memory freed by `close()`, measured on the WASM memory; worker-thread
sessions), the browser tests, and the crash projects (sessions, and the plain call's cache, after a
crash).

**The scale:** exactly one of `zoom` or `metersPerPixel`; both or neither is an `InputError`
with code `BAD_OPTION`. `zoom` (0–24) counts 512-pixel tiles, as MapLibre GL, Mapbox GL and deck.gl
do; a 256-pixel-tile map (Leaflet, OpenLayers, Google Maps) passes its zoom − 1. `metersPerPixel`
is ground meters per screen pixel at the middle of the routes' bounding box: `2π × 6378137 ×
cos(latitude) / (tileSize × 2^zoom)` on a Web Mercator map. `src/scale.test.ts` checks a zoom and
its meters per pixel draw the same bits (at the equator; within 1e-12° elsewhere), and Leaflet's
conversion.

`routes` is a GeoJSON FeatureCollection of up to 70 LineStrings in `[lng, lat]`, each with a
unique GeoJSON `id` (more limits below). `lines` has one LineString per route (same id, direction and properties);
`stations` holds one bar per station when the optional `stations` option (GeoJSON Points) is
given. Bad input rejects with an `InputError` naming the route, station or option (below). The work runs in one shared Web Worker, for plain calls and sessions alike.

**Public names.** Every entry (`transit-braid`, `transit-braid/node`, `transit-braid/inline`)
exports the same list: `src/public.ts` (the errors `BraidError`, `InputError` and `InternalError`, the
defaults `DEFAULT_LINE_WIDTH` and `DEFAULT_SPACING`, and the types), plus its own `sideBySide()`,
`createSession()` and `createBraidWorker()`. Anything else is internal. `src/public.test.ts` pins the list (and the
inline e2e test the inline file's). The errors pass `instanceof` across two copies of the package
(the inline file next to the main entry, or two versions): each carries its kinds under a symbol
from the global registry (`Symbol.for`), and each class's `Symbol.hasInstance` reads them.

**Errors.** Every rejection is a `BraidError` with a stable `code` (compare the code, not the
message): an `InputError` when the input can't be drawn, an `InternalError` when transit-braid
itself failed (its message starts "Internal error in transit-braid, not a problem with your
input"), or a plain `BraidError` with code `TERMINATED` for a call on a worker you terminated. An
`InputError` also says which route, station or option is at fault, where that applies: `routeId`
(the GeoJSON id), `stationId`, `index` (its place in `features`), `point` (its place in the
route's `coordinates`) and `option`. The WASM refuses with a `Refusal` object (code, reason, and
the route, point or station index); TypeScript names the route or station from its own list, so
no message is rewritten (`refusalError` in `src/errors.ts`). One test per code:
`src/error-codes.test.ts` (real WASM; the crash projects test `ENGINE_CRASHED` and
`WORKER_FAILED`).

| Code                   | Class           | Details                     | When                                                                |
| ---------------------- | --------------- | --------------------------- | ------------------------------------------------------------------- |
| `BAD_OPTION`           | `InputError`    | `option`                    | an option is missing, of the wrong type or out of range             |
| `BAD_ROUTES`           | `InputError`    | `index`, `routeId`          | not a FeatureCollection of LineString features                      |
| `MISSING_ID`           | `InputError`    | `index`                     | a route without a usable GeoJSON `id`                               |
| `DUPLICATE_ID`         | `InputError`    | `routeId`, `index`          | two routes with one id (`index` of the second)                      |
| `TOO_MANY_ROUTES`      | `InputError`    |                             | more than 70 routes                                                 |
| `BAD_COORDINATE`       | `InputError`    | `routeId`, `index`, `point` | a point that isn't `[lng, lat]` numbers on the map                  |
| `TOO_FEW_POINTS`       | `InputError`    | `routeId`, `index`          | fewer than two distinct points                                      |
| `TOO_MANY_POINTS`      | `InputError`    | `routeId`, `index`          | over 1,000,000 points together (the route that passes it)           |
| `CROSSES_ANTIMERIDIAN` | `InputError`    | `routeId`, `index`, `point` | a jump of more than 180° of longitude (`point` after it)            |
| `TOO_WIDE`             | `InputError`    |                             | the routes spread over 200 km east-west or north-south              |
| `TOO_LONG`             | `InputError`    |                             | over 2,000 km of routes together                                    |
| `TOO_COMPLEX`          | `InputError`    |                             | valid input, too much work (the work budget below)                  |
| `BAD_STATION`          | `InputError`    | `index`, `stationId`        | not a FeatureCollection of Points, or a station off the map         |
| `TOO_MANY_STATIONS`    | `InputError`    |                             | more than 5,000 stations                                            |
| `ENGINE_CRASHED`       | `InternalError` |                             | the WASM crashed and restarted (crash recovery below)               |
| `WORKER_FAILED`        | `InternalError` |                             | the worker failed and was stopped                                   |
| `INTERNAL`             | `InternalError` |                             | anything else that should never happen (an answer that doesn't fit) |
| `TERMINATED`           | `BraidError`    |                             | a call on a worker you terminated                                   |

```sh
pnpm test                                        # all TS tests
pnpm exec vitest run packages/transit-braid      # just this package
pnpm exec vitest options                         # watch mode, files matching "options"
```

- Tests sit next to the code as `*.test.ts` (e.g. `src/options.test.ts`).

**Input limits and work budget.** Valid input that is too big or too hard to draw is
refused at once with an `InputError` naming the limit, so no call runs for minutes or runs
out of memory. The core checks them all (`crates/transit-braid-core/src/pipeline/limits.rs` and
`budget.rs`); TypeScript checks the point count again before copying the routes for the worker.

| Limit                                                   | Bound           | Our cities' largest |
| ------------------------------------------------------- | --------------- | ------------------- |
| Routes                                                  | 70              | 50                  |
| Points, all routes together                             | 1,000,000       | 16,890              |
| Neighbouring points across the date line (> 180° apart) | none            | 0.034°              |
| Spread of all routes, east-west and north-south         | 200 km each way | 39 km               |
| Length of all routes together                           | 2,000 km        | 1,020 km            |
| Stations                                                | 5,000           | 1,875               |
| Nearby-segment checks (topology; points piled up)       | 20 million      | 2.5 million         |
| Graph steps (topology; many short crossing pieces)      | 50 million      | 6.7 million         |
| Station checks (stations × centerline segments)         | 50 million      | 12.5 million        |

The work bounds count steps, not time, so a refusal is the same on every machine and after any
earlier call (an edit counts the whole graph, as a fresh call does), and a refused call leaves
the cache as it was. Tests: `cargo test -p transit-braid-core -- refused budget reach always_drawn`
and `pnpm exec vitest run --project node node-entry -t "input limits"` (the same refusals in WASM).

**Crash recovery.** A crash inside the WASM (a trap: a Rust panic, or no memory or stack
left) fails only that call, with an `InternalError` (`ENGINE_CRASHED`) whose message starts "Internal error in
transit-braid, not a problem with your input". `run.ts` swaps the crashed instance for a fresh one
at once (`__wbg_reset_state()` from the wasm-bindgen glue), so the next call works and the crashed
memory is freed. The fresh instance has no caches: every session's next call, an edit too, draws as
a first call would. A worker that fails (an error event, its thread stops) is stopped and its waiting calls
reject; `sideBySide()` starts a new shared worker on the next call, while a call on your own
`createBraidWorker()` rejects at once, as after `terminate()`.

The crash tests make the WASM really run out of memory: two Vitest projects cap each WebAssembly
memory at 8 MB (V8's `--wasm-max-mem-pages`), in Node (`*.crash.test.ts`) and in Chromium
(`*.crash.browser.test.ts`), and give it 50 routes of 2000 points (about 17 MB). No test-only
code: the tests run the production `.wasm`.

```sh
pnpm build:wasm && pnpm exec vitest run --project crash --project crash-browser
```

### WebAssembly bridge — `crates/transit-braid-wasm`

Exposes the pipeline to JavaScript; only what the package calls. Data crosses as flat typed
arrays. One entry point, `sideBySide(coords, starts, routeKeys, stations, stationGroups, zoom,
metersPerPixel, lineWidth, spacing, maxSideBySide)`, runs the whole pipeline
(`transit-braid-core/src/pipeline/`; native and WebAssembly give the same bits): `coords`
is flat `[lng, lat, ...]`, `starts` a `Uint32Array` of where each route starts (in points),
`maxSideBySide` the lane cap or `undefined`. It returns a `SideBySideResult` whose arrays
JavaScript takes out once each (`takeCoords()`, `takeStarts()`, `takeBars()`, `takeDropped()`):
each moves out of Rust's memory instead of being cloned there first. A refused call throws a `Refusal` (`code`, `reason`, and the
`route`, `point` or `station` index at fault; `src/refusal.rs`), never a message to parse.
Real city networks of 50 lines take 70–110 ms in Node.

```sh
pnpm build:wasm                                  # build + print size (budget: 300 KB gzipped)
cargo test -p transit-braid-wasm                 # Rust-side tests (no JS engine needed)
pnpm exec vitest run wasm.node                   # TS calls the real .wasm in Node
```

How the build works (`scripts/build-wasm.sh`):

1. `cargo build --locked --target wasm32-unknown-unknown --profile wasm-release`: Rust → `.wasm`, with
   SIMD on (`.cargo/config.toml`) and speed optimizations (`[profile.wasm-release]` in `Cargo.toml`;
   measured faster _and_ smaller than size optimizations for this code).
2. `wasm-bindgen --target web --experimental-reset-state-function`: writes the JS loader and
   `.d.ts` types. Works in browsers and Node. The flag adds `__wbg_reset_state()`, which crash
   recovery uses; the `.wasm` is the same without it.
3. `wasm-opt -O3`: optimizes the module for speed and strips metadata.
4. Fails if the gzipped `.wasm` is over the budget (`WASM_BUDGET_KB`, default 300).

The output folder `packages/transit-braid/wasm/` is generated and git-ignored. The
`wasm-bindgen` CLI version must equal the `wasm-bindgen` crate version; `scripts/install-tools.sh`
pins both and refuses to run if they differ.

### Web Worker in a real browser (Chromium) — `src/worker.ts`, `src/client.ts`

The heavy work runs in a Web Worker so the page never freezes. `createBraidWorker()` starts one of
your own (`sideBySide()` and `createSession()` use a shared one); each call is a Promise, and
`worker.sideBySide()` and `worker.createSession()` are the same calls on that worker. Inside, the worker loads the `.wasm` via `new URL(..., import.meta.url)`,
which every modern bundler understands.

```sh
pnpm exec vitest run --project browser           # Chromium, headless
pnpm exec vitest run --project smoke             # Firefox and WebKit: one sideBySide() call each
pnpm exec vitest --project browser --browser.headless=false   # watch the tests run in a browser
```

- Browser tests are named `*.browser.test.ts`; the Firefox/WebKit smoke test `*.smoke.test.ts`;
  the crash tests (above) `*.crash.test.ts` (Node) and `*.crash.browser.test.ts` (Chromium);
  everything else (`*.test.ts`) runs in Node.
- Messages follow `src/protocol.ts`: each request has an `id`, and the answer carries the same
  `id`, so many calls can run at once. Results come back as transferred buffers (no copy).

### Inline entry — `transit-braid/inline`

For bundlers that can't follow `new URL(..., import.meta.url)` (they don't copy the worker or the
`.wasm`). Same API as `transit-braid`; everything is inside one ES module: the worker's script,
started from a Blob URL, and the WASM as base64, compiled inside the worker. About 345 KB
(145 KB gzipped), against 237 KB for the `.wasm` alone.

```ts
import { sideBySide } from 'transit-braid/inline';
```

```sh
pnpm build:wasm && pnpm build:packages          # writes packages/transit-braid/dist/inline.js
pnpm exec playwright test inline.spec           # Chromium: only that file served, no new URL
```

Vite builds it, as the last step of the package's build (`packages/transit-braid/vite.config.ts`).
`src/inline.ts` uses Vite's `?worker&inline` and `?inline` imports (declared in
`src/inline-env.d.ts`), so it is only ever used built: in this repo the package's `./inline`
export points types at the source and code at `dist/inline.js`.

### Node entry — `transit-braid/node`

`sideBySide()` for Node 24+ (scripts, servers, tests): same input, output and errors, on the
caller's own thread, so a call blocks it while it runs. The WASM is read from
`packages/transit-braid/wasm/` and compiled on the first call. A server whose thread must not
block starts its own worker: `createBraidWorker()` runs on `worker_threads` and keeps the process
alive only while calls wait. Its thread gets none of the process's Node flags (`src/node-port.ts`):
Node refuses a worker given a V8 flag such as `--max-old-space-size`. In this repo the export
points at the TypeScript source, which Node 24 runs as is; the published package
points at `dist/node.js` (see Packaging below).

```ts
import { createBraidWorker, createSession, sideBySide } from 'transit-braid/node';

const lines = await sideBySide(routes, { zoom: 14 }); // this thread
const session = createSession(); // this thread, with a memory
const worker = createBraidWorker(); // a worker thread
const same = await worker.sideBySide(routes, { zoom: 14 });
const there = worker.createSession(); // a session in the worker thread
worker.terminate();
```

```sh
pnpm build:wasm && pnpm exec vitest run node-entry   # also runs it in a plain node process
```

### Packaging — `pnpm build:packages`, `pnpm test:pack`

Both npm packages build into their own `dist/`: tsc writes the JavaScript and the `.d.ts` files
(`tsconfig.build.json`, `rewriteRelativeImportExtensions` turns `./x.ts` imports into `./x.js`),
then, for `transit-braid`, Vite writes the one-file `dist/inline.js`. No source maps. The built
code loads the WASM from the package's `wasm/` folder (`../wasm/` from `dist/`, as from `src/`).

```sh
pnpm build:wasm && pnpm build:packages           # packages/*/dist/
pnpm test:pack                                   # pack, install, use (about 2 s)
cd packages/transit-braid && pnpm pack --dry-run # what the tarball would hold
```

- **Two `exports` maps.** In this repo `exports` points at the TypeScript source, so the demo,
  the tests and Node run it without a build. `publishConfig.exports` is what ships: `pnpm pack`
  and `pnpm publish` put it in the packed `package.json` (npm itself doesn't: release from the
  tarball `pnpm pack` makes). The published `transit-braid` maps `.` and `./inline` to the Node
  entry under the `node` condition, so a server-side import works too.
- **Publishing:** `publishConfig` asks for public access and provenance; npm makes provenance
  only in CI, so the release workflow publishes (§7).
- **`files`** lists only what ships: `dist/`, the three WASM files, plus the README, LICENSE and
  `package.json` npm always adds. Tests, sources and fixtures never ship.
- **`transit-braid-deck`** takes `transit-braid` and deck.gl as peer dependencies; pnpm turns
  `workspace:^` into a real range when it packs.
- **The pack test** (`scripts/pack.spec.ts`, its own `playwright.pack.config.ts`, no server)
  packs both, unpacks and checks each tarball (only what ships, every export target present, no
  `workspace:` versions, `private` still set, no path from this machine in any file), installs
  the tarballs into a throwaway project in the temp folder (pnpm, no install scripts, the
  workspace's release age and trust policy; deck.gl at this workspace's version), and then: Node 24 runs
  `transit-braid/node` (a call, a session, a worker thread, an `InputError`); tsc checks the
  installed types in a browser project and a Node-only project without `skipLibCheck`, and the
  deck layer's; and Vite bundles a page with the default entry, the inline entry and
  `BraidLayer`, served to Chromium, which draws with each (its worker script and `.wasm` load as
  their own files).

### Demo map — `examples/demo`

A web page with a MapLibre map (OpenFreeMap tiles, no API key) and a side panel.
`sideBySide()` draws the routes side by side, with station bars and a small arrow every so often
showing each route's direction. Every change calls it again, through one session:

- **Network**: three made-up routes in Tel Aviv that share a trunk, then split (the default, to
  edit); 25 real bus routes in Tel Aviv with their 711 stops (`public/data/*.geojson`, from the
  Israel Ministry of Transport's GTFS; each file says its source, license and credit, and the page
  shows the credit with the network); or one of the 12 test scenarios from `fixtures/` (made-up
  routes in meters, drawn around 0°, 0° on a plain background, with what to look for).
  `?network=<id>` opens a network, and the address follows the picker.
- **Examples page** (`examples.html`, `src/examples.ts`): a card per network with a small picture
  of its routes as given (`src/thumbnail.ts`), what it shows and what to look for; each card opens
  the demo with it.
- **Side by side** (switch): off shows the routes as given, on top of each other (before).
- **Zoom** (slider) and **Lane cap** (`maxSideBySide`: whole routes left out where more than that
  many share a street; the panel says how many).
- **Remove** a route with its ✕ in the list.
- **Move** a route: click its name, then drag its points on the map (drawn again on release;
  live dragging comes later).
- **Add route**: click points on the map, then **Finish** (at least two different points);
  **Cancel** drops it.
- **Zoom**: widths are in pixels but offsets are in meters, so zooming draws again.

The edit logic and reading routes from GeoJSON are in `src/edit.ts`, the networks in
`src/networks.ts`, the scenarios in `src/scenarios.ts`, the direction arrows in `src/chevrons.ts`
and the Examples page's pictures in `src/thumbnail.ts` (each tested in Node, `*.test.ts`).

```sh
pnpm build:wasm && pnpm demo        # then open http://localhost:5199
                                    # add ?basemap=none for a plain background
pnpm e2e                            # end-to-end tests (starts the demo itself)
pnpm exec playwright show-report    # open the last report, with the screenshot
```

- The e2e tests (`examples/demo/e2e/*.spec.ts`) don't compare screenshots pixel by pixel
  (rendering differs between macOS, Linux and GPUs). They ask deck.gl which line is drawn at a
  point known to lie on each line. "Before" and "after" screenshots are attached to the report.
- The demo exposes `window.braidDebug` (map, overlay, network, routes, draws started, the route
  being moved, points of a route being added, status, probe points, bars, arrows, routes the cap
  left out) for the tests.
- deck.gl draws on its own canvas above the map (`interleaved: false`): deck.gl 9.4's
  interleaved mode breaks with MapLibre 6.

**The demo site** (published to GitHub Pages from `main` by `pages.yml`, and by every release,
§7): both pages built as a static
site (`vite build --mode site`, `examples/demo/dist-site/`), every URL relative, and every import
of `transit-braid` (the deck layer's too) replaced by `transit-braid/inline`, so the worker and the
WASM come inside one file. `pnpm e2e:site` runs the demo's, the deck page's and the Examples
page's e2e tests on that build, served by `vite preview` on port 5197.

```sh
pnpm build:wasm && pnpm build:packages && pnpm build:site   # examples/demo/dist-site/
pnpm e2e:site                                               # the demo, deck and Examples tests on it
```

### deck.gl layer — `packages/transit-braid-deck`

`BraidLayer`, a deck.gl `CompositeLayer`:

```ts
new BraidLayer({ id, data: routes, stations, getColor, lineWidth: 4, spacing: 1, pickable: true });
```

`data` is the same GeoJSON `sideBySide()` takes. The layer owns a session (`createSession()`,
closed when the layer is removed) and calls it when its props change and once the zoom has stayed still for 150 ms (`ZOOM_SETTLE_MS`); the last good result stays
on screen meanwhile, and only the newest request's answer is drawn. A failed call keeps the last
good lines and goes to deck.gl's `onError`. It draws one `PathLayer` for the lines (default color:
the route's `properties.color`, else gray) and one for the station bars. Picking a line gives the
output feature: the route's id and properties. deck.gl is a peer dependency.

```sh
pnpm build:wasm && pnpm demo        # then open http://localhost:5199/deck.html
pnpm exec playwright test deck.spec # its end-to-end tests (colors per lane, errors, zoom)
pnpm exec vitest run packages/transit-braid-deck   # the color helper, in Node
```

The demo page (`examples/demo/deck.html`, `src/deck/main.ts`) draws three routes on deck.gl's own
canvas with no base map, so the test can read exact colors; it exposes `window.deckDebug`.

### Test kit — `fixtures/`, `crates/transit-braid-testkit`

Test networks: small hand-made cases, one per hard situation (Y-split, hairpin, ...), and a
70-route stress grid. Format and list: [fixtures/README.md](fixtures/README.md). The testkit crate (never
published) holds what the tests need to prove the library on them: the fixture loader and its
checks, the hand-built expected graphs, the golden harness (below), the quality rules Q1–Q8 with
their gate on every committed fixture (`src/quality.rs`) and the clean-graph criteria C1–C5
with the hand-made topology test (`src/criteria.rs`, `src/built.rs`, C4).

```sh
cargo test -p transit-braid-testkit      # every fixture loads and validates; the quality gate; C4
```

- To add a fixture: drop a `*.geojson` file in `fixtures/synthetic/` (or `fixtures/<city>/`),
  with `braid.name` equal to the file name. The test above picks it up automatically, and the
  golden test below fails until you add its digests (`BRAID_GOLDEN_UPDATE=1`).
- Every bug we find becomes a fixture before it is fixed.

### Golden harness — `fixtures/golden.json`

**In plain words:** we drew every test map once and saved a short fingerprint of each drawing.
Every test run draws them again and compares the fingerprints. If anything moves, even by a
hair, its fingerprint changes and the test fails, so a drawing can't change by accident. When we
change one on purpose, we save new fingerprints and say why in the PR.

Fingerprints of everything Braid draws, so a refactor can prove "same output, bit for bit". For
every committed fixture (`synthetic/`, `bus/`, `stress/`), with station bars: each whole zoom
10–19 and the fractional zooms 12.5, 15.37 and 17.75, each from a fresh cache, the lane cap
(`maxSideBySide`) 1, 2 and 3, and a scripted edit sequence through one cache (move a point, remove
a route, add it back, reverse the order, zoom). The TypeScript test runs each sequence through a
fresh session (`createSession()`), so a session's edits are checked against the same digests. Each output gets two 64-bit FNV-1a digests over
every coordinate's exact bits: `lines` (ids, points, dropped routes) and `bars`. Inputs and
format: `crates/transit-braid-testkit/src/golden.rs`.

```sh
cargo test -p transit-braid-testkit --test golden    # native, about 7 s
pnpm build:wasm && pnpm exec vitest run golden        # the same digests from sideBySide() in WebAssembly
```

- A failure names every digest that changed (`stops zooms.15 bars: expected …, got …`).
- **Changed on purpose:** `BRAID_GOLDEN_UPDATE=1 cargo test -p transit-braid-testkit --test
  golden` rewrites the file and prints what changed. Run the TypeScript test too, then commit the
  file's diff with the change and say why in the PR. Without the variable the test never writes.
- Fractional zooms have the same bits natively and in WebAssembly: their scale comes
  from the core's own `det::exp2` (only `+ − × ÷`), not the platform's math library.
- The Rust side also checks, while digesting, that a zoom-only rerun gives a fresh run's bits and
  that the incremental topology equals a full build at every edit step.

## 4. Project layout

```
crates/                   the Cargo workspace (root Cargo.toml): core, wasm, testkit
  transit-braid-testkit/  what the tests need (never published): loads + validates fixtures
    src/graph.rs          hand-built expected graphs (*.graph.json)
    src/golden.rs         golden digests of the pipeline's output (tests/golden.rs)
    src/place.rs          a fixture in [lng, lat] for the pipeline, or in meters
    src/quality.rs        quality rules Q1–Q8 and their gate on the committed fixtures
    src/criteria.rs       clean-graph criteria C1–C5
    src/built.rs          the core's topology on a fixture, as a graph (C4 compares it)
    src/despike.rs        the criteria's copy of the core's spike removal
  transit-braid-wasm/     WebAssembly bindings (wasm-bindgen) around the core
  transit-braid-core/     Rust algorithms (pure, no WASM/JS)
    src/geom.rs           points, segments, offset_segment, a polyline's length
    src/polyline.rs       offset_polyline: continuous offset line, mitered corners (and
                          offset_polyline_within: outside corners beveled past an overshoot)
    src/pipeline/         side_by_side: [lng, lat] lines in → side-by-side lines out:
      mod.rs              the output (Drawn) and the plain entry points
      settings.rs         the map scale and line widths (Settings, Scale)
      limits.rs           the input limits and the checks every call makes first
      error.rs            PipelineError: why a call was refused
      projection.rs       [lng, lat] ↔ ground meters, meters per pixel at a zoom
      cache.rs            what a call keeps for the next (Cache: zoom-only reruns, edits)
      cap.rs              the lane cap (side_by_side_capped)
      *_tests.rs          the tests of each part (test_support.rs: shared settings and inputs)
    src/budget.rs         the work budget: nearby checks, graph steps, station checks
    src/geometry/         every line in its slot along each graph edge, node fronts and
                          smooth connectors joining them along its walk:
      mod.rs              slots, the lines along every walk, corners rounded
      fronts.rs           how far each node cuts its edges back
      walk.rs             one route along its walk: pieces in its slot, connectors
      loops.rs            cutting a connector's loop and offsetting's folds
      stations.rs         station bars, same-name stations merged
      *_tests.rs          the tests of each part (test_support.rs: shared graphs)
    src/det.rs            cos, exp and exp2 with the same bits on every platform
    src/ordering/         line ordering: cost (crossings, separations), exhaustive search,
                          greedy descent + annealing for large parts:
      mod.rs              weights, settings, the solution, solve() and the split into parts
      cost.rs             how lines pass through a node (chords) and what an order costs
      exhaustive.rs       every order of a small part
      search.rs           the search's state: swaps costed on the nodes they touch; descent
      anneal.rs           simulated annealing from a fixed seed
      chains.rs           edges joined at pass-through nodes share one order
      warm.rs             the order after an edit, from the order before
      *_tests.rs          the tests of each part (test_support.rs: shared graphs and inputs)
    src/topology/         routes → line graph and
                          each route's walk through it (build_with_traversals):
      mod.rs              the graph's types, settings, errors and the full build
      keys.rs             steps 0–2: spikes removed, samples, each sample's key; flicker smoothing
      grid.rs             the segment grid the keys look up, and an edit's reach
      assemble.rs         steps 3–4: runs → edges → nodes, and each route's walk
      split.rs            step 3b: an edge cut where a route joins or leaves it
      path.rs             measuring along a path (nearest point, cut, simplify)
      incremental.rs      the incremental build after an edit
      matching.rs         which routes an edit kept, by id or by place
      *_tests.rs          the tests of each part (test_support.rs: shared inputs)
examples/
  demo/                   Vite + MapLibre + deck.gl demo map
    src/main.ts           draws routes with sideBySide(); add, remove and move them
    src/edit.ts           the demo's routes and edits (pure, tested in Node)
    src/deck/main.ts      the BraidLayer page (deck.html)
    e2e/demo.spec.ts      Playwright end-to-end tests (deck.spec.ts: BraidLayer)
packages/
  transit-braid/          TypeScript core package (npm `transit-braid`; README.md, LICENSE)
    tsconfig.build.json   the build's tsc settings (dist/: JavaScript and .d.ts)
    vite.config.ts        the one-file build of transit-braid/inline (dist/inline.js)
    src/side-by-side.ts   sideBySide() and createSession(): the shared worker (side-by-side-on.ts:
                          checks, runs it)
    src/session.ts        sessions: a cache kept in the worker between one session's calls
    src/inline.ts         transit-braid/inline: the same API, worker and WASM inside one file
    src/node.ts           transit-braid/node: sideBySide() and createSession() on the caller's
                          thread; createBraidWorker()
    src/node-worker.ts    Node's worker_threads worker (node-wasm.ts loads the .wasm from disk;
                          node-port.ts starts the thread and talks to it)
    src/index.ts          transit-braid: public.ts plus the worker-backed sideBySide()
    src/public.ts         the public names every entry exports
    src/routes.ts         GeoJSON in and out: checks routes, flattens them, builds the result
    src/errors.ts         BraidError, InputError, InternalError and their codes (instanceof
                          across copies); the engine's refusals named by route or station
    src/options.ts        resolveOptions (internal): defaults + validation
    src/wasm.node.test.ts loads the real .wasm in Node and calls Rust
    src/worker.ts         Web Worker: loads the .wasm from a URL (inline-worker.ts: from base64)
    src/serve.ts          the worker's side: answers requests (shared by both workers)
    src/run.ts            calls the Rust function a request names (the worker's and Node's);
                          swaps a crashed WASM instance for a fresh one
    src/client.ts         createBraidWorker(): starts the worker from a URL
    src/web-port.ts       a browser Worker as the connection's port (node-port.ts: Node's)
    src/connection.ts     main-thread side, Promises (+ the pipeline call; shared by both entries);
                          stops a failed worker; sharedWorker() starts a new one
    src/protocol.ts       message shapes between the two
    wasm/                 generated by pnpm build:wasm (git-ignored)
    dist/                 generated by pnpm build:packages (git-ignored)
  transit-braid-deck/     deck.gl adapter: BraidLayer (npm `transit-braid-deck`)
    src/braid-layer.ts    the CompositeLayer: sideBySide() on change and settled zoom
    src/color.ts          default colors from properties.color
scripts/
  pack.spec.ts            pnpm test:pack: the packed packages installed and used
  lint-workflows.sh       pnpm lint:workflows: zizmor on the workflows (offline; --online in CI)
  stress-grid.ts          writes fixtures/stress/stress-grid.geojson (its test checks the file)
  install-tools.sh        pinned + checksum-verified native tools
  build-wasm.sh           Rust → WebAssembly build with size budget
fixtures/                 test networks (format in fixtures/README.md)
  synthetic/              six hand-made cases, each with its expected graph (*.graph.json)
  bus/                    out-and-back and one-way street pair, with expected graphs
  stress/                 the 70-route stress grid (scripts/stress-grid.ts)
  golden.json             golden digests of every committed fixture
.github/workflows/ci.yml  CI: Rust, TypeScript, secrets, workflow lint, dependency licenses and advisories
.github/workflows/release.yml  release on a v* tag: npm (staged), GitHub release, demo site
.github/workflows/scorecard.yml  OpenSSF Scorecard, weekly and on main (public repository only)
.github/dependabot.yml    weekly dependency update PRs (actions, Rust crates, npm packages)
AGENTS.md                 rules and findings for contributors and AI agents
CONTRIBUTING.md           the short start: five commands, the folders the build makes
DEVELOPMENT.md            this guide
docs/design/              design notes (topology, ordering), with a picture per step (img/)
docs/img/                 the README's pictures, drawn from real output
CHANGELOG.md              what each version of the npm packages changed (Keep a Changelog)
SECURITY.md               how to report a vulnerability, supported versions, what's in scope
```

Config files at the root: `Cargo.toml` (Rust workspace), `rust-toolchain.toml` (Rust version),
`deny.toml` (allowed licenses), `clippy.toml` (banned methods: platform math, and slow ones in WASM), `package.json` + `pnpm-workspace.yaml` (JS workspace),
`tsconfig.json`, `vitest.config.ts`, `playwright.config.ts` (and `playwright.pack.config.ts`, the pack test's), `.oxlintrc.json`, `.oxfmtrc.json`, `lefthook.yml`.

## 5. Commit hooks and CI

**On every commit** (lefthook, installed by `pnpm install`), on the files you staged:

- oxfmt formats TS/JSON/Markdown/YAML and re-stages the result
- oxlint lints TS files with type information; a warning blocks the commit
- rustfmt formats the staged Rust files (in either Cargo workspace) and re-stages the result
- gitleaks scans the staged changes for secrets (API keys, tokens, private keys)

Run them by hand: `pnpm exec lefthook run pre-commit`.

**On every PR and every push to `main`** (GitHub Actions, `.github/workflows/ci.yml`):

| Job                                      | Runs                                                                                                                                             |
| ---------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------ |
| Rust lint and tests                      | `cargo fmt --check`, `clippy -D warnings` (native + WASM), `cargo test`                                                                          |
| TypeScript format, lint, types and tests | Builds the WASM, then `pnpm check:ts` (incl. e2e and the pack test; e2e report uploaded)                                                         |
| Secret scan (full history)               | `gitleaks git` over every commit                                                                                                                 |
| Workflow lint (zizmor)                   | `scripts/lint-workflows.sh --online`: zizmor on every workflow and `dependabot.yml`, with the audits that ask GitHub (the job's read-only token) |
| Dependency licenses and advisories       | `cargo deny --locked check` (Rust), `pnpm audit --prod` (npm)                                                                                    |

Wait for all green checks before merging.

**What runs by itself** (all free for a public repository):

| What                          | When                                                                           | Does                                                                                                                    |
| ----------------------------- | ------------------------------------------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------- |
| CI (`ci.yml`)                 | every PR and every push to `main`                                              | the checks above                                                                                                        |
| Release (`release.yml`)       | a version tag (`v*`) is pushed                                                 | builds, publishes both npm packages, drafts the GitHub release with signed files, updates the demo on GitHub Pages (§7) |
| Demo site (`pages.yml`)       | a push to `main` that changes the demo, the packages or the core (public only) | builds and tests the demo site, then updates it on GitHub Pages once a maintainer approves                              |
| Scorecard (`scorecard.yml`)   | weekly and on every push to `main` (public only)                               | grades the repository's security habits (below)                                                                         |
| Dependabot (`dependabot.yml`) | weekly                                                                         | opens PRs that update dependencies; never merges by itself. GitHub's own bot, on once its file is in the repo           |

**Supply-chain safety:**

- CI's GitHub token has no permissions by default (`permissions: {}`); each job may only read the
  repo (`contents: read`), and checkout doesn't leave the token on disk (`persist-credentials: false`).
- Every action is pinned to an exact commit (the tag is in a comment) and the runner image to
  `ubuntu-24.04`; every job has a time limit; cargo runs `--locked`; a new push to a PR cancels
  its older runs.
- Native tools come from `scripts/install-tools.sh`: pinned versions, each download's SHA-256
  checked (cargo-deny too, not a container that downloads it unchecked).
- zizmor lints every workflow (`pnpm lint:workflows`, and a CI job): unpinned actions,
  template injection, risky triggers, broad permissions, kept credentials, cache poisoning,
  Dependabot without a cooldown, and more. A finding fails the check. Locally it runs offline;
  CI's job runs it online (`scripts/lint-workflows.sh --online`, with the job's `GITHUB_TOKEN`,
  which may only read), adding the audits that ask GitHub: a pinned commit that isn't in the
  action's own repository (impostor commits), a version comment that doesn't match its pin, a
  ref that names both a branch and a tag, a pin to a commit no release tags, and known-vulnerable
  action versions. To accept one on purpose, add a `# zizmor: ignore[<audit>]` comment on that
  line, with the reason.
- Dependabot opens grouped update PRs weekly for actions, Rust crates and npm packages, each
  version at least 7 days old (its cooldown); security updates don't wait.
- pnpm refuses npm versions published less than 7 days ago (`minimumReleaseAge` in
  `pnpm-workspace.yaml`), which blocks most hijacked releases before they are caught, and versions
  published with less trust than the ones before them, for example without the provenance the
  earlier versions had (`trustPolicy: no-downgrade`; each exception in `trustPolicyExclude` says
  why). Both rules are checked against the lockfile on every install. Install scripts are off by
  default.
- Check by hand: `pnpm audit` and `cargo deny --locked check`.
- [OpenSSF Scorecard](https://scorecard.dev) (`.github/workflows/scorecard.yml`) scores the
  repository's practices (pinned actions, token permissions, branch protection, …) on every push
  to `main` and weekly, publishes the score to scorecard.dev and adds its findings to GitHub code
  scanning. It runs only in the public repository once that is public (the `BRAID_PUBLIC`
  variable, §7); anywhere else the job is skipped. Scorecard refuses published results unless
  that file keeps its rules: no top-level `env` or `defaults`, no `run` steps, only its listed
  actions in the job.
- To report a vulnerability: [SECURITY.md](SECURITY.md).

**The tools, in plain words:**

- **gitleaks** looks for secrets (keys, tokens, passwords) in commits: before each commit, and
  over the whole history in CI. Nothing in this repository needs a secret: even publishing to
  npm uses none (trusted publishing, below).
- **zizmor** checks our GitHub automation files for security mistakes, such as a workflow that
  leaks its token or runs text from a PR title as a command.
- **cargo-deny** checks every Rust dependency's license and its known security advisories;
  **pnpm audit** checks the npm packages against known advisories.
- **Clippy** is Rust's code checker; **`clippy.toml`** holds its settings. Ours bans a few math
  functions that are slow in WebAssembly or give slightly different results on different
  machines.
- **OpenSSF Scorecard** grades the repository's security habits. OpenSSF is the Open Source
  Security Foundation, part of the Linux Foundation.
- **Dependabot** is GitHub's bot that proposes dependency updates.
- **Trusted publishing:** npm accepts a release from this repository's release job by the job's
  signed identity, so no npm password or token is stored anywhere.

## 6. Workflow: branches, commits, PRs

1. Agree on the change with the maintainer first (an issue or a discussion). One change = one PR.
2. Branch from `main`: `git switch -c fix-station-bars`.
3. Make **small commits** with [conventional](https://www.conventionalcommits.org/) subjects
   (`feat: add X`, `fix: resolve Y`, `test:`, `docs:`, `chore:`, `ci:`), plus at most one or two
   plain-English lines.
4. Follow the code rules:
   - **No breaking changes:** don't change, rename or delete a public function, type or export
     unless the maintainer asked for it.
   - **No new dependencies** without the maintainer's explicit permission.
   - **Tests are integration or property-based tests**, with no stubs or mocks. Focus on null
     values, empty payloads, malformed input and error handling.
5. Run `pnpm check` before pushing.
6. **Security review** of your diff (`git diff main`) before pushing:
   - **Versions:** new or bumped dependencies are current and maintained; `pnpm audit` and
     `cargo deny --locked check` pass; no new install scripts; actions pinned to a commit SHA.
   - **Injection:** no `eval`/`new Function`; untrusted input never reaches shell commands,
     HTML/SVG, file paths or `${{ }}` in workflows; no `unsafe` Rust.
   - **Credentials:** no keys, tokens or `.env` files (gitleaks also checks on commit and in CI).
   - **CI permissions:** every workflow starts with `permissions: {}` and each job asks for the
     least it needs (`contents: read` unless it truly needs more); every checkout has
     `persist-credentials: false`; no `pull_request_target` or `workflow_run` triggers; values
     anyone can set (branch names, PR titles, `github.event.*`) never go inside `${{ }}` in a
     `run:` step (pass them through `env:`); every job has `timeout-minutes` and runs on
     `ubuntu-24.04`; `pnpm lint:workflows` (zizmor) passes.
   - **Packages:** when a PR touches what ships, look at `pnpm pack --dry-run` in each package:
     only `dist/`, the WASM files, README, LICENSE and `package.json`; `pnpm test:pack` also
     fails on a test file, a source map or a path from this machine in a tarball.
7. Push and open a PR. GitHub fills in the template (`.github/pull_request_template.md`):
   **Architectural Why**, **Blast Radius** ("Trunk" = core architecture, types or security;
   "Leaf" = isolated helper) and **Data Flow**, then the maintainer's review checklist.
   Keep each part short.
8. Merge when CI is green. Update this guide in the same PR if you added a part or command.
9. **Versions:** both npm packages and the Rust workspace share one version (0.1.1 now, set in
   `packages/*/package.json` and the root `Cargo.toml`). A PR that changes what the packages do
   or export says so in `CHANGELOG.md`: in the unpublished version's entry until it is released,
   then under `[Unreleased]`.

## 7. Releasing

`release.yml` runs only in the public repository, and only while its `BRAID_PUBLIC` variable is
`true`. The packages reach npm only from it, through npm's trusted
publishing: no npm token exists anywhere, npm links each version to the run that built it
(provenance), and each version is staged first and goes public only when a maintainer approves
it with 2FA. The Rust crates are not published.

**What a `v*` tag runs** (`.github/workflows/release.yml`; no job may write unless it says so):

1. **build** (read-only, no caches): checks the tag is both packages' version and that
   `CHANGELOG.md` has a dated entry for it; `pnpm check`; `pnpm pack` of both packages (pnpm
   applies `publishConfig`; npm wouldn't), a CycloneDX SBOM of each (`pnpm sbom`) and
   `SHA256SUMS`, passed on as an artifact (and the checksums as the job's output); the demo site
   (`pnpm build:site`, tested by `pnpm e2e:site`).
2. **attest** (after build; `id-token: write` and `attestations: write`, the only job that may
   attest): no checkout and no install, so no project code runs with those permissions;
   downloads the artifact, checks it against the build's checksums, and signs build provenance
   for the tarballs and SBOMs (`actions/attest-build-provenance`; "Checking a release file",
   below).
3. **publish-npm** (after build and attest; environment `npm`: waits for a maintainer to approve
   the deployment; `id-token: write` only): checks the tarballs against the build's checksums,
   then `npm stage publish` of each, `transit-braid` first.
4. **github-release** (after build and attest; `contents: write`): a draft release with the
   tarballs, SBOMs and `SHA256SUMS`, its notes from the version's CHANGELOG entry.
5. **pages** (after build; environment `github-pages`: waits for a maintainer to approve the
   deployment; `pages: write`, `id-token: write`): deploys the site.

Nothing is staged on npm or put on the GitHub release unless its files were attested; the demo
site isn't a release file, so it doesn't wait for the attestation.

Safety locks, each enough to stop a publish: the repository and variable check on every job;
npm trusts only this repository's `release.yml` in the `npm`
environment, which waits for a maintainer; and every staged version waits for a maintainer's 2FA.

**Each release:**

1. A PR sets the version in both `packages/*/package.json` and the root `Cargo.toml`, and dates
   its CHANGELOG entry (`## [0.2.0] - 2027-01-15`). The package READMEs (what npm shows) load
   their picture from that version's tag, so the PR also moves `v0.1.1` in their image URL to the
   new tag; the picture is `docs/img/before-after.png`, a 2x PNG of the README's SVG (npm shows
   no relative or SVG images), drawn again whenever the SVG changes.
2. After it merges, a signed tag on `main`: `git tag -s v0.2.0 -m "transit-braid 0.2.0"`, then
   `git push origin v0.2.0`.
3. Approve the run's `npm` and `github-pages` deployments (Actions → the run → Review
   deployments).
4. Approve each staged version with 2FA: `npm stage list`, `npm stage view <id>` (its files and
   provenance), `npm stage approve <id>`.
5. Publish the draft GitHub release.

**Checking a release file.** Every tarball and SBOM on the GitHub release has a signed build
provenance attestation: which repository, workflow, tag and commit built it. With the
[GitHub CLI](https://cli.github.com), on a downloaded file:

```sh
gh attestation verify transit-braid-0.2.0.tgz --repo shakedk/transit-braid \
  --signer-workflow shakedk/transit-braid/.github/workflows/release.yml \
  --source-ref refs/tags/v0.2.0 --deny-self-hosted-runners
```

It fails unless the file's SHA-256 has an attestation signed by this repository's
`release.yml` on that tag, on a GitHub-hosted runner. `sha256sum --check SHA256SUMS` checks the
files against each other; the attestation checks where they came from. (The npm packages carry
npm's own provenance: `npm audit signatures`.)

**Once, before the first release** (repository settings, then npm):

- Repository settings: the variable `BRAID_PUBLIC` = `true` once the repository is public
  (Settings → Secrets and variables → Actions → Variables; it also turns on the Scorecard
  workflow); an environment `npm` with the maintainer as required reviewer and
  deployments from `v*` tags only; GitHub Pages built by "GitHub Actions", its `github-pages`
  environment with the maintainer as required reviewer and deployments from `main` and `v*`
  tags; immutable releases; a ruleset that protects `v*` tags.
- The first version is published by hand, because npm sets up trusted publishing only for a
  package that already exists ([`npm trust`][npm-trust]). Reject the first run's `npm`
  deployment, download its `release` artifact, check it (`sha256sum --check SHA256SUMS`), then,
  logged in with 2FA (`npm login`):

  ```sh
  # From a laptop, npm can't make provenance, so turn off what publishConfig asks for.
  npm stage publish --provenance=false ./transit-braid-0.1.0.tgz   # creates the package
  npm stage publish --provenance=false ./transit-braid-deck-0.1.0.tgz
  npm stage list                                     # the stage ids
  npm stage approve <id>                             # each, with 2FA
  ```

  Then, on npmjs.com, each package's Settings → Publishing access → "Require two-factor
  authentication and disallow tokens" ([trusted publishing][npm-trusted] keeps working: it
  uses short-lived OIDC credentials, not tokens). The first version has no provenance (it came
  from a laptop); every later one does.

- On the day of the next release, connect npm to the workflow, in each package's folder (npm
  takes the repository from its `package.json`):

  ```sh
  npm trust github --file release.yml --env npm --allow-stage-publish
  npm trust list                                     # check it
  ```

  A new trusted publisher has to publish within 2 days ([npm][npm-trusted]), so set it up when
  you tag. `--allow-stage-publish` lets the workflow stage, never publish directly.

npm's docs, checked on 2026-11-24: [trusted publishing][npm-trusted] (updated 2026-10-06; npm
11.5.1+ and Node 22.14+, `id-token: write`, provenance automatic from GitHub Actions for a public
repository and package), [`npm stage`][npm-stage] (npm 12.2.3, edited 2026-09-24; staging can
create a new package, approving needs 2FA, OIDC tokens can stage but not approve) and
[`npm trust`][npm-trust] (npm 12.2.3, edited 2026-06-02; the package must exist, the account
needs 2FA). npm's publishing rules are still changing: check them again before each first step.

[npm-trusted]: https://docs.npmjs.com/trusted-publishers
[npm-stage]: https://docs.npmjs.com/cli/v12/commands/npm-stage
[npm-trust]: https://docs.npmjs.com/cli/v12/commands/npm-trust

## 8. Troubleshooting

| Problem                                                     | Fix                                                                                                                                                     |
| ----------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `node`/`pnpm` not found in a script or editor               | nvm loads in `~/.zshrc`/`~/.zprofile`; open a new terminal, or run `source ~/.zprofile`                                                                 |
| `cargo` not found                                           | `source ~/.cargo/env` (rustup adds this to `~/.zshenv`)                                                                                                 |
| `pnpm install` warns about ignored builds                   | Expected: pnpm 12 blocks install scripts by default; allowed ones are listed in `pnpm-workspace.yaml`                                                   |
| Commit blocked by the hook                                  | Read the lint message, fix, `git add`, commit again                                                                                                     |
| First `cargo` command is slow                               | rustup is installing the pinned Rust version once                                                                                                       |
| `Cannot find module '../wasm/transit_braid_wasm.js'`        | Run `pnpm build:wasm` (the WASM output is generated, not committed)                                                                                     |
| `Missing wasm-bindgen` / `wasm-opt` / `gitleaks`            | Run `pnpm setup:tools`                                                                                                                                  |
| `pnpm lint:workflows`: "Found zizmor … but … pins"          | Another zizmor comes first on your `PATH`: run `pnpm setup:tools` and put `~/.local/bin` first                                                          |
| `pnpm lint:workflows` reports a finding                     | Each finding links its audit's page: fix the workflow; accept it only with a `# zizmor: ignore[<audit>]` comment and the reason                         |
| `cargo deny --version` isn't the pinned one                 | An older `cargo install` copy comes first: `cargo uninstall cargo-deny`, then `pnpm setup:tools`                                                        |
| Install fails: a version is within `minimumReleaseAge`      | It is under 7 days old: wait, or for an urgent security fix add it to `minimumReleaseAgeExclude`                                                        |
| Install fails: `ERR_PNPM_TRUST_DOWNGRADE`                   | A version came with less trust than the ones before it: check its release; if genuine, add exactly that version to `trustPolicyExclude` with the reason |
| `the lock file needs to be updated but --locked was passed` | Run `cargo update -w` (or the `cargo build` that changed it) and commit `Cargo.lock`                                                                    |
| Commit blocked: "leaks found"                               | gitleaks found a secret in your staged changes. Remove it; if it was real, rotate it                                                                    |
| Browser tests fail: "Executable doesn't exist"              | Run `pnpm exec playwright install chromium firefox webkit`                                                                                              |
| `Port 5199 is in use`                                       | Another demo server is running; stop it, or reuse it (`pnpm e2e` does that locally)                                                                     |
