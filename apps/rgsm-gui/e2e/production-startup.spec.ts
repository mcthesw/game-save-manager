import { test, expect } from '@playwright/test';
import { join } from 'node:path';
import { build, preview } from 'vite';
import { DEVICE_A_ID, GAME_NAME } from './support/constants';
import { seedLocalConfig, writeSaveText } from './support/local-fixture';
import { closeResources } from './support/process';
import {
  createRunRoot,
  removeRunRoot,
  startRgsmHost,
  workspacePath,
} from './support/rgsm-instance';

test('production bundle boots and loads lazy routes against the real host', async ({ browser }) => {
  const runRoot = await createRunRoot('production-startup');
  const appRoot = workspacePath('apps', 'rgsm-gui');
  const outDir = join(runRoot, 'dist');
  const closers: Array<() => Promise<unknown>> = [];
  const pageErrors: string[] = [];
  let failed = false;
  try {
    await build({ root: appRoot, build: { outDir, emptyOutDir: false }, logLevel: 'warn' });
    const device = await seedLocalConfig(runRoot);
    await writeSaveText(device.savePath, 'production-save\n');
    const host = await startRgsmHost({
      appDataDir: device.appDataDir,
      deviceId: DEVICE_A_ID,
      logPath: join(runRoot, 'logs', 'host.log'),
    });
    closers.push(host.stop);
    const server = await preview({
      root: appRoot,
      build: { outDir },
      preview: {
        host: '127.0.0.1',
        port: 0,
        open: false,
        proxy: {
          '/api/v1': {
            target: host.apiBaseUrl,
            configure(proxy) {
              // Same-origin test proxy forwards authenticated requests to the real
              // host, without extending the desktop host's trusted origin list.
              proxy.on('proxyReq', (request) => request.removeHeader('origin'));
            },
          },
        },
      },
    });
    closers.push(() => server.close());
    const address = server.httpServer.address();
    if (!address || typeof address === 'string') throw new Error('Preview has no TCP address');
    const origin = `http://127.0.0.1:${address.port}`;
    const context = await browser.newContext({ baseURL: origin });
    closers.push(() => context.close());
    await context.addInitScript(
      (runtime) => {
        window.__RGSM_RUNTIME__ = runtime;
      },
      { apiBaseUrl: origin, token: host.token }
    );
    const page = await context.newPage();
    page.on('pageerror', (error) => pageErrors.push(error.message));
    await page.goto('/');
    await expect(page.getByRole('button', { name: 'Cloud sync', exact: true })).toBeVisible();
    for (const [route, heading] of [
      ['/Settings', 'Preferences'],
      ['/SyncSettings', 'Sync settings'],
      [`/Management/${encodeURIComponent(GAME_NAME)}`, GAME_NAME],
    ]) {
      await page.goto(route);
      await expect(page.getByRole('heading', { name: heading, exact: true })).toBeVisible();
    }
    await expect(page.getByRole('button', { name: 'Create new snapshot' })).toBeVisible();
    expect(pageErrors).toEqual([]);
  } catch (error) {
    failed = true;
    if (pageErrors.length) {
      throw new Error(`Production bundle errors: ${pageErrors.join('; ')}`, { cause: error });
    }
    throw error;
  } finally {
    await closeResources(closers);
    if (!failed) await removeRunRoot(runRoot);
  }
});
