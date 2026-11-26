# Contributing

Thanks for helping. This page takes you from a fresh clone to a passing check in five commands.
Everything else is in [DEVELOPMENT.md](DEVELOPMENT.md).

## Set up (once)

You need git, Node 24, pnpm 12 and Rust (rustup picks the exact Rust version by itself).
[DEVELOPMENT.md §1](DEVELOPMENT.md#1-one-time-setup) has the install command for each.
`pnpm setup:tools` (below) puts its tools in `~/.local/bin`, so that folder must be on your
`PATH`; if it isn't, run this once and open a new terminal:

```sh
echo 'export PATH="$HOME/.local/bin:$PATH"' >> ~/.zshrc
```

```sh
git clone https://github.com/shakedk/transit-braid.git && cd transit-braid
pnpm install                 # JavaScript packages, and the commit hooks
pnpm setup:tools             # pinned native tools (wasm-bindgen, wasm-opt, gitleaks, cargo-deny, zizmor), checked, into ~/.local/bin
pnpm exec playwright install chromium firefox webkit   # the test browsers
pnpm check                   # everything CI checks; about two minutes once the caches are warm
```

If `pnpm check` passes, you're set up.

## Folders the build creates

None of these is committed; git ignores them, and deleting one is always safe (the next build
makes it again).

| Folder                                | What it holds                                      | Made by               |
| ------------------------------------- | -------------------------------------------------- | --------------------- |
| `node_modules/`                       | JavaScript dependencies                            | `pnpm install`        |
| `target/`                             | Rust build output                                  | any `cargo` command   |
| `packages/transit-braid/wasm/`        | the WebAssembly module and its JavaScript loader   | `pnpm build:wasm`     |
| `packages/*/dist/`                    | the built npm packages                             | `pnpm build:packages` |
| `examples/demo/dist-site/`            | the demo built as a static site (for GitHub Pages) | `pnpm build:site`     |
| `test-results/`, `playwright-report/` | what the browser tests leave behind                | `pnpm e2e`            |

## See it work

`pnpm demo` starts the demo map at http://localhost:5199: drag the map, zoom, and add or move a
route to see the lines move apart.

## Making a change

1. For anything bigger than a typo, open an issue first, so we agree on the change.
2. One small change per pull request, with a test that proves it. Tests run the real code (no
   fakes or mocks) and try bad input first: empty, missing, malformed.
3. Run `pnpm check` before you push, and fill in the pull request template.
4. If your change alters what the library draws, the golden digests fail on purpose: see
   [DEVELOPMENT.md](DEVELOPMENT.md#golden-harness--fixturesgoldenjson).

The full rules, for people and AI agents alike: [AGENTS.md](AGENTS.md).
