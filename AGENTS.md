# Notes for contributors and AI agents

Read this first, then [DEVELOPMENT.md](DEVELOPMENT.md) (setup, commands, workflow in §6).

## What this is

A library that takes overlapping bus, tram or rail routes as GeoJSON lines (each directed, with
its own id and its own slot) and returns GeoJSON: one offset line per input route, plus
station bars. The core is Rust compiled to WebAssembly, run in a Web Worker in browsers or on the
caller's thread in Node 24+. TypeScript API: `sideBySide()` and `createSession()`; the deck.gl
layer `BraidLayer` is in `transit-braid-deck`. City scale: at most 70 routes (more is rejected
with an error), real street geography. MIT, written only from the published papers (Bast, Brosi,
Storandt). LOOM is GPL-3.0: never copy its code.

Published names: npm `transit-braid` and `transit-braid-deck`.

## Rules (the maintainer reviews architecture, API contracts and test failures, not syntax)

1. **No breaking changes:** don't change, rename or delete a public function, type or export
   unless asked.
2. **No new dependencies** (npm, cargo or CI actions) without explicit permission.
3. **Tests:** integration or property-based, no stubs or mocks. Focus on null values, empty
   payloads, malformed input and error handling.
4. **One small step per PR**, each ending with a test that proves it.
5. **Commits:** small, conventional subject (`feat:`, `fix:`, `test:`, `docs:`, `chore:`,
   `ci:`) plus at most two plain-English lines.
6. **PR body:** use `.github/pull_request_template.md` (Architectural Why, Blast Radius
   Trunk/Leaf, Data Flow, then the maintainer's checklist, left unticked).
7. **Before every push:** `pnpm check`, then the security review in DEVELOPMENT.md §6
   (versions/supply chain, injection, secrets, CI permissions). Update DEVELOPMENT.md when you
   add a part, command or tool.
8. **The drawing is pinned:** `fixtures/golden.json` holds digests of everything the library
   draws for every committed fixture. A change that alters the output regenerates it on purpose
   (`BRAID_GOLDEN_UPDATE=1`, DEVELOPMENT.md) and says why in the PR.

## Technical findings

- Chromium is the full test target. Firefox and WebKit get one smoke test (load the worker, one
  `sideBySide()` call; `*.smoke.test.ts`). Nothing more there.
- Never use `f64::hypot` or `f64::mul_add` in Rust: slow in WASM; `clippy.toml` bans them.
- In the pipeline, never decide on a platform-math result (`cos`, `exp`, `exp2`, `atan2`, `ln`,
  `tan`…): native and WASM differ in the last bit. Use `det.rs` or exact comparisons.
  `clippy.toml` bans them in production code; tests may use them.
- Production crates (core, wasm) never `unwrap`, `expect`, `panic!` or print (strict lints in the
  root `Cargo.toml`); an exception is `#[expect(lint, reason = "…")]`, never a bare `#[allow]`.
- Production WASM uses `opt-level = 3` (faster and smaller than `"z"` for this code).
- MapLibre zoom uses 512 px tiles.
- deck.gl 9.4's interleaved mode breaks with MapLibre 6.
