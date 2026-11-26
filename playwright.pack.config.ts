import { defineConfig, devices, type PlaywrightTestConfig } from '@playwright/test';

// The pack test (`pnpm test:pack`, scripts/pack.spec.ts): both packages packed, installed from
// their tarballs into a throwaway project and used as an app would, in Node and in Chromium.
// It needs the built packages (`pnpm build:packages`) and no server.
const config: PlaywrightTestConfig = defineConfig({
  testDir: 'scripts',
  testMatch: /pack\.spec\.ts/,
  outputDir: 'test-results/pack',
  forbidOnly: Boolean(process.env['CI']),
  reporter: [['list']],
  timeout: 60_000,
  use: { ...devices['Desktop Chrome'] },
});

export default config;
