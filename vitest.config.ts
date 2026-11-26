import { playwright } from '@vitest/browser-playwright';
import { defineConfig, type ViteUserConfig } from 'vitest/config';

// Three kinds of tests:
//   node    — *.test.ts, fast, run in Node
//   browser — *.browser.test.ts, run in real Chromium (headless): the full browser suite
//   smoke   — *.smoke.test.ts, run in Firefox and WebKit: one sideBySide() call each
// and crash tests, in Node (*.crash.test.ts) and Chromium (*.crash.browser.test.ts), where
// each WebAssembly memory may grow to 8 MB only (V8's flag), so a big input really crashes the
// WASM (out of memory, a trap like any Rust panic) and the tests see Braid recover.

/**
 * Each test's time limit. CI's machines are slower than a laptop: the session tests that draw the
 * 70-route stress grid again and again take 3–4 s on a laptop and over Vitest's default 5 s there.
 */
const TEST_TIMEOUT_MS = 30_000;

/** The crash tests' WebAssembly memory cap, in 64 KB pages: 8 MB (a small call needs 1.3 MB). */
const WASM_CAP_PAGES = 128;
const capFlag = `--wasm-max-mem-pages=${WASM_CAP_PAGES}`;

const config: ViteUserConfig = defineConfig({
  test: {
    projects: [
      {
        test: {
          name: 'node',
          testTimeout: TEST_TIMEOUT_MS,
          include: [
            'packages/*/src/**/*.test.ts',
            'examples/*/src/**/*.test.ts',
            'scripts/**/*.test.ts',
          ],
          exclude: [
            '**/*.browser.test.ts',
            '**/*.smoke.test.ts',
            '**/*.crash.test.ts',
            '**/node_modules/**',
          ],
        },
      },
      {
        test: {
          name: 'browser',
          testTimeout: TEST_TIMEOUT_MS,
          include: ['packages/*/src/**/*.browser.test.ts'],
          exclude: ['**/*.crash.browser.test.ts', '**/node_modules/**'],
          browser: {
            enabled: true,
            headless: true,
            provider: playwright(),
            instances: [{ browser: 'chromium' }],
          },
        },
      },
      {
        test: {
          name: 'smoke',
          include: ['packages/*/src/**/*.smoke.test.ts'],
          browser: {
            enabled: true,
            headless: true,
            provider: playwright(),
            instances: [{ browser: 'firefox' }, { browser: 'webkit' }],
          },
        },
      },
      {
        test: {
          name: 'crash',
          include: ['packages/*/src/**/*.crash.test.ts'],
          execArgv: [capFlag],
        },
      },
      {
        test: {
          name: 'crash-browser',
          include: ['packages/*/src/**/*.crash.browser.test.ts'],
          browser: {
            enabled: true,
            headless: true,
            provider: playwright({ launchOptions: { args: [`--js-flags=${capFlag}`] } }),
            instances: [{ browser: 'chromium' }],
          },
        },
      },
    ],
  },
});

export default config;
