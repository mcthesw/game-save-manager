import { defineConfig } from '@playwright/test';

export default defineConfig({
  testDir: '.',
  testMatch: 'docs-screenshots.capture.ts',
  workers: 1,
  timeout: 180_000,
  expect: { timeout: 20_000 },
  use: { baseURL: 'http://localhost:5173', headless: true },
  globalSetup: '../e2e/support/global-setup.ts',
  outputDir: '../../../.rgsm-dev/docs-capture-results',
  reporter: 'list',
});
