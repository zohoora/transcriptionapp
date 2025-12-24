import { defineConfig, devices } from '@playwright/test';

/**
 * Playwright configuration for visual regression testing.
 *
 * These tests capture screenshots of the app in various states
 * and compare them against baseline images to detect unintended
 * visual changes.
 */
export default defineConfig({
  testDir: './specs',
  outputDir: './test-results',

  // Snapshot configuration
  snapshotDir: './snapshots',
  snapshotPathTemplate: '{snapshotDir}/{testFilePath}/{arg}{ext}',

  // Update snapshots with: npx playwright test --update-snapshots
  updateSnapshots: 'missing',

  // Fail on first failure for faster feedback
  fullyParallel: false,
  workers: 1,

  // Retry failed tests once
  retries: 1,

  // Reporter configuration
  reporter: [
    ['html', { outputFolder: './playwright-report' }],
    ['list'],
  ],

  // Global timeout
  timeout: 30000,

  // Expect configuration for screenshots
  expect: {
    toHaveScreenshot: {
      // Allow small differences due to anti-aliasing
      maxDiffPixelRatio: 0.01,
      // Threshold for color difference
      threshold: 0.2,
    },
  },

  use: {
    // Base URL for the dev server
    baseURL: 'http://localhost:1420',

    // Capture screenshot on failure
    screenshot: 'only-on-failure',

    // Record trace on failure
    trace: 'on-first-retry',

    // Viewport size
    viewport: { width: 800, height: 600 },
  },

  projects: [
    {
      name: 'Desktop Chrome',
      use: { ...devices['Desktop Chrome'] },
    },
    {
      name: 'Desktop Safari',
      use: { ...devices['Desktop Safari'] },
    },
  ],

  // Run dev server before tests
  webServer: {
    command: 'pnpm dev',
    url: 'http://localhost:1420',
    reuseExistingServer: !process.env.CI,
    timeout: 120000,
  },
});
