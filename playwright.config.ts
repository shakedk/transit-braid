import { defineConfig, devices, type PlaywrightTestConfig } from '@playwright/test';

// End-to-end tests: start the demo app and check it in a real Chromium. With BRAID_E2E_SITE=1
// (`pnpm e2e:site`) they check the built demo site instead (`pnpm build:site`: the inline entry,
// relative URLs), served as a static site is.
const site = process.env['BRAID_E2E_SITE'] === '1';
const url = site ? 'http://localhost:5197' : 'http://localhost:5199';

const config: PlaywrightTestConfig = defineConfig({
  testDir: 'examples/demo/e2e',
  outputDir: 'test-results',
  forbidOnly: Boolean(process.env['CI']),
  retries: process.env['CI'] ? 1 : 0,
  reporter: [['list'], ['html', { open: 'never' }]],
  use: {
    baseURL: url,
    viewport: { width: 1100, height: 750 },
    trace: 'retain-on-failure',
  },
  projects: [
    {
      name: 'chromium',
      // SwiftShader = software WebGL, so maps render on CI machines without a GPU.
      use: {
        ...devices['Desktop Chrome'],
        launchOptions: { args: ['--enable-unsafe-swiftshader'] },
      },
    },
  ],
  webServer: {
    command: `pnpm --filter transit-braid-demo ${site ? 'preview:site' : 'dev'}`,
    url,
    // The site's tests never reuse a server: it must be this build.
    reuseExistingServer: !site && !process.env['CI'],
  },
});

export default config;
