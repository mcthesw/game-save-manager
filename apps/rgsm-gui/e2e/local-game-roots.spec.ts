import { test, expect } from '@playwright/test';
import { readFile, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { DEVICE_A_ID } from './support/constants';
import { seedLocalConfig } from './support/local-fixture';
import { openApp } from './support/gui';
import {
  createRunRoot,
  hostPost,
  newDeviceContext,
  removeRunRoot,
  startRgsmHost,
  type RgsmHost,
} from './support/rgsm-instance';
import type { Config } from '../src/api/commands';

test('auto-detection merges existing Windows root aliases and preserves bindings across reload', async ({
  browser,
}) => {
  const runRoot = await createRunRoot('game-roots');
  const device = await seedLocalConfig(runRoot);
  const configPath = join(device.appDataDir, 'GameSaveManager.config.json');
  const config = JSON.parse(await readFile(configPath, 'utf8'));
  config.devices[DEVICE_A_ID].resources = [
    { id: 2, source: 'manual', kind: { type: 'gameRoot', store: 'steam', path: 'C:/Steam' } },
    { id: 5, source: 'detected', kind: { type: 'gameRoot', store: 'steam', path: 'c:\\steam\\' } },
    {
      id: 6,
      source: 'manual',
      kind: {
        type: 'gameInstallation',
        root_id: 5,
        store: 'steam',
        install_dir: 'Test',
        path: 'C:/Steam/steamapps/common/Test',
      },
    },
  ];
  config.devices[DEVICE_A_ID].next_resource_id = 9;
  config.games[0].device_bindings = { [DEVICE_A_ID]: { rootIds: [5] } };
  await writeFile(configPath, JSON.stringify(config));
  let host: RgsmHost | undefined;
  let context;
  let failed = false;
  try {
    host = await startRgsmHost({
      appDataDir: device.appDataDir,
      deviceId: DEVICE_A_ID,
      logPath: join(runRoot, 'host.log'),
    });
    const started = await newDeviceContext(browser, host);
    context = started.context;
    const page = started.page;
    // Only OS discovery is controlled; editing, saving and reloading use the real host.
    await page.route('**/api/v1/detect-game-roots', (route) =>
      route.fulfill({
        json: ['C:/Steam/', 'c:\\steam', 'D:/Games', 'd:\\games\\'],
      })
    );
    await openApp(page);
    await page.getByRole('button', { name: 'Settings', exact: true }).click();
    await page.getByRole('button', { name: 'Auto Scan', exact: true }).click();
    const roots = page.getByRole('textbox', { name: 'e.g. D:\\Steam', exact: true });
    await expect(roots).toHaveCount(2);
    for (let attempt = 0; attempt < 2; attempt++) {
      const saved = page.waitForResponse((response) =>
        response.url().endsWith('/api/v1/set-config')
      );
      await page.getByRole('button', { name: 'Auto-detect', exact: true }).click();
      expect((await saved).ok()).toBe(true);
      await expect(roots).toHaveCount(2);
      await expect(roots.nth(0)).toHaveValue('C:/Steam');
      await expect(roots.nth(1)).toHaveValue('D:/Games');
    }
    const stored = await hostPost<Config>(host, '/api/v1/get-local-config');
    expect(stored.ok, stored.raw).toBe(true);
    const storedDevice = stored.data.devices![DEVICE_A_ID]!;
    expect(storedDevice.resources!.map((resource) => resource.id)).toEqual([2, 6, 9]);
    expect(storedDevice.next_resource_id).toBe(10);
    expect(stored.data.games[0]!.device_bindings?.[DEVICE_A_ID]?.rootIds).toEqual([2]);
    const installation = storedDevice.resources!.find((resource) => resource.id === 6)!;
    expect(installation.kind.type === 'gameInstallation' && installation.kind.root_id).toBe(2);
    await page.reload();
    await page.getByRole('button', { name: 'Auto Scan', exact: true }).click();
    await expect(roots).toHaveCount(2);
    await expect(roots.nth(1)).toHaveValue('D:/Games');
    await page.screenshot({
      path: test.info().outputPath('game-roots.png'),
      animations: 'disabled',
    });
  } catch (error) {
    failed = true;
    throw error;
  } finally {
    await context?.close();
    await host?.stop();
    if (!failed) await removeRunRoot(runRoot);
  }
});
