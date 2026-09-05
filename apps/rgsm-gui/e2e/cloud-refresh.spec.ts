import { expect, test } from '@playwright/test';
import { seedEmptyCloudWithLocalGame } from './support/cloud-fixture';
import {
  createLibrary,
  connectLibrary,
  createPublishedSnapshot,
  openGame,
  openSyncSettings,
  snapshotRow,
} from './support/gui';
import { createRunRoot, hostPost } from './support/rgsm-instance';
import { startDualSession } from './support/session';
import { addGameViaApi, getLocalGame } from './support/local-gui';
import { DEVICE_A_ID, GAME_NAME } from './support/constants';

test('an invalidated connection refresh cannot overwrite the new configuration', async ({
  browser,
}) => {
  const runRoot = await createRunRoot('refresh-connection');
  const seeded = await seedEmptyCloudWithLocalGame(runRoot);
  const session = await startDualSession(browser, {
    ...seeded,
    runRoot,
    label: 'refresh-connection',
  });
  let release = () => {};
  const gate = new Promise<void>((resolve) => {
    release = resolve;
  });
  let captured = false;
  let failed = false;
  try {
    await createLibrary(session.pageA);
    await session.pageA.evaluate(async () => {
      const modulePath = '/src/composables/useCloudLibrary.ts';
      await (await import(modulePath)).refreshCloudLibrary();
    });
    await session.pageA.route('**/api/v1/get-current-device-game-statuses', async (route) => {
      if (captured) return route.continue();
      const response = await route.fetch();
      captured = true;
      await gate;
      await route.fulfill({ response });
    });
    const oldRefresh = session.pageA.evaluate(async () => {
      const modulePath = '/src/composables/useCloudLibrary.ts';
      // Start the intercepted read after any refresh already started by navigation.
      await (await import(modulePath)).refreshCloudLibrary(true);
    });
    await expect.poll(() => captured, { timeout: 10_000 }).toBe(true);
    await addGameViaApi(session.hostA, 'New connection game', [
      { type: 'File', path: seeded.deviceA.savePath },
    ]);
    const fresh = await session.pageA.evaluate(async () => {
      const libraryPath = '/src/composables/useCloudLibrary.ts';
      const configPath = '/src/composables/useConfig.ts';
      const { clearCloudLibrary, refreshCloudLibrary } = await import(libraryPath);
      clearCloudLibrary();
      await refreshCloudLibrary();
      const { config, deviceGameStatuses } = (await import(configPath)).useConfig();
      return {
        games: config.value.games,
        devices: config.value.devices,
        statuses: deviceGameStatuses.value,
      };
    });
    expect(fresh.games.map((game: { name: string }) => game.name)).toContain('New connection game');
    release();
    await oldRefresh;
    const after = await session.pageA.evaluate(async () => {
      const modulePath = '/src/composables/useConfig.ts';
      const { config, deviceGameStatuses } = (await import(modulePath)).useConfig();
      return {
        games: config.value.games,
        devices: config.value.devices,
        statuses: deviceGameStatuses.value,
      };
    });
    expect(after).toEqual(fresh);
  } catch (error) {
    failed = true;
    throw error;
  } finally {
    release();
    await session.close(failed);
  }
});

for (const editor of ['managed files', 'auto-save settings'] as const) {
  test(`focus refresh preserves the open ${editor} draft until it is saved`, async ({
    browser,
  }) => {
    const runRoot = await createRunRoot('refresh-editor');
    const seeded = await seedEmptyCloudWithLocalGame(runRoot);
    const session = await startDualSession(browser, {
      ...seeded,
      runRoot,
      label: 'refresh-editor',
    });
    const { pageA: page, hostA: host } = session;
    let failed = false;
    try {
      await createLibrary(page);
      await openGame(page);
      if (editor === 'managed files') {
        await page.getByRole('button', { name: 'View managed files' }).click();
      } else {
        await page.getByRole('button', { name: 'More actions' }).click();
        await page.getByRole('menuitem', { name: 'Auto-save settings' }).click();
      }
      const drawer = page.getByRole('dialog');
      const changedPath = `${seeded.deviceA.savePath.replaceAll('\\', '/')}.edited`;
      const path = drawer.locator('[contenteditable="true"]').last();
      const timer = drawer
        .getByRole('heading', { name: 'Auto Backup', exact: true })
        .locator('xpath=ancestor::section[1]')
        .getByRole('switch')
        .first();
      if (editor === 'managed files') {
        await expect(path).toHaveText(seeded.deviceA.savePath.replaceAll('\\', '/'));
        await path.fill(changedPath);
      } else {
        await timer.click();
        await drawer.getByRole('spinbutton', { name: 'Max auto backups' }).fill('7');
      }
      await page.evaluate(() => window.dispatchEvent(new Event('focus')));
      await page.evaluate(async () => {
        const modulePath = '/src/composables/useCloudLibrary.ts';
        await (await import(modulePath)).refreshCloudLibrary();
      });
      if (editor === 'managed files') {
        await expect(path).toHaveText(changedPath);
        await drawer.getByRole('button', { name: 'save', exact: true }).click();
        await expect
          .poll(async () => {
            const unit = (await getLocalGame(host, GAME_NAME)).save_paths[0];
            return unit.source.type === 'concrete' ? unit.source.paths[DEVICE_A_ID] : null;
          })
          .toBe(changedPath);
      } else {
        await expect(timer).toBeChecked();
        await expect(drawer.getByRole('spinbutton', { name: 'Max auto backups' })).toHaveValue('7');
        await drawer.getByRole('button', { name: 'Save settings', exact: true }).click();
        await expect
          .poll(async () => (await getLocalGame(host, GAME_NAME)).auto_backup)
          .toEqual({ interval_secs: 300, max_backup_count: 7 });
      }
    } catch (error) {
      failed = true;
      throw error;
    } finally {
      await session.close(failed);
    }
  });
}

test('returning to an open game refreshes cloud snapshots without clearing known data on failure', async ({
  browser,
}) => {
  const runRoot = await createRunRoot('cloud-refresh');
  const seeded = await seedEmptyCloudWithLocalGame(runRoot);
  const session = await startDualSession(browser, { ...seeded, runRoot, label: 'cloud-refresh' });
  let failed = false;
  try {
    await createLibrary(session.pageA);
    const first = await createPublishedSnapshot(session.pageA, session.hostA, 'First');
    await connectLibrary(session.pageB);
    await openGame(session.pageB);
    await expect(snapshotRow(session.pageB, first)).toBeVisible();
    const next = await createPublishedSnapshot(session.pageA, session.hostA, 'New on A');
    await session.pageB.evaluate(() => window.dispatchEvent(new Event('focus')));
    await expect(snapshotRow(session.pageB, next)).toBeVisible({ timeout: 10_000 });
    let reads = 0;
    await session.pageB.route('**/api/v1/refresh-cloud-archive-library', async (route) => {
      reads += 1;
      await route.continue();
    });
    await session.pageB.evaluate(async () => {
      const modulePath = '/src/composables/useCloudLibrary.ts';
      const { refreshCloudLibrary } = await import(modulePath);
      await Promise.all([refreshCloudLibrary(), refreshCloudLibrary(), refreshCloudLibrary()]);
    });
    expect(reads).toBe(1);
    await session.pageB.unroute('**/api/v1/refresh-cloud-archive-library');
    await session.pageB.route('**/api/v1/refresh-cloud-archive-library', (route) =>
      route.fulfill({
        status: 503,
        contentType: 'application/json',
        body: JSON.stringify({ code: 'unavailable', message: 'Cloud unavailable' }),
      })
    );
    const failedRefresh = session.pageB.waitForResponse(
      (response) =>
        new URL(response.url()).pathname === '/api/v1/refresh-cloud-archive-library' &&
        response.status() === 503
    );
    await session.pageB.evaluate(() => window.dispatchEvent(new Event('focus')));
    await failedRefresh;
    await expect(snapshotRow(session.pageB, first)).toBeVisible();
    await expect(snapshotRow(session.pageB, next)).toBeVisible();
    await openSyncSettings(session.pageB);
    await expect(
      session.pageB.getByText('Could not refresh cloud data, showing the last known information', {
        exact: true,
      })
    ).toBeVisible();
    await expect(session.pageB.getByText('Synced', { exact: true })).toHaveCount(0);
  } catch (error) {
    failed = true;
    throw error;
  } finally {
    await session.close(failed);
  }
});

test('a failed configuration refresh preserves the loaded games and their ownership', async ({
  browser,
}) => {
  const runRoot = await createRunRoot('config-refresh');
  const seeded = await seedEmptyCloudWithLocalGame(runRoot);
  const session = await startDualSession(browser, { ...seeded, runRoot, label: 'config-refresh' });
  let failed = false;
  try {
    await createLibrary(session.pageA);
    await createPublishedSnapshot(session.pageA, session.hostA, 'Local remains');
    const before = await session.pageA.evaluate(async () => {
      const modulePath = '/src/composables/useConfig.ts';
      const { useConfig } = await import(modulePath);
      return {
        games: useConfig().config.value.games,
        statuses: useConfig().deviceGameStatuses.value,
      };
    });
    await session.pageA.route('**/api/v1/get-local-config', (route) =>
      route.fulfill({
        status: 503,
        contentType: 'application/json',
        body: JSON.stringify({ code: 'unavailable', message: 'Read failed' }),
      })
    );
    const after = await session.pageA.evaluate(async () => {
      const modulePath = '/src/composables/useConfig.ts';
      const { useConfig } = await import(modulePath);
      const ok = await useConfig().refreshConfig();
      return {
        ok,
        games: useConfig().config.value.games,
        statuses: useConfig().deviceGameStatuses.value,
      };
    });
    expect(after.ok).toBe(false);
    expect(after.games).toEqual(before.games);
    expect(after.statuses).toEqual(before.statuses);
  } catch (error) {
    failed = true;
    throw error;
  } finally {
    await session.close(failed);
  }
});

test('refresh after a mutation does not reuse a response read before that mutation', async ({
  browser,
}) => {
  const runRoot = await createRunRoot('refresh-order');
  const seeded = await seedEmptyCloudWithLocalGame(runRoot);
  const session = await startDualSession(browser, { ...seeded, runRoot, label: 'refresh-order' });
  let release = () => {};
  const responseGate = new Promise<void>((resolve) => {
    release = resolve;
  });
  let readStarted = false;
  let failed = false;
  try {
    await createLibrary(session.pageA);
    await createPublishedSnapshot(session.pageA, session.hostA, 'Before');
    await connectLibrary(session.pageB);
    await openGame(session.pageB);
    let reads = 0;
    await session.pageB.route('**/api/v1/refresh-cloud-archive-library', async (route) => {
      reads += 1;
      if (reads !== 1) return route.continue();
      const response = await route.fetch();
      readStarted = true;
      await responseGate;
      await route.fulfill({ response });
    });
    await session.pageB.evaluate(async () => {
      const modulePath = '/src/composables/useCloudLibrary.ts';
      const { refreshCloudLibrary } = await import(modulePath);
      // The gated read must start after interception, not reuse navigation's read.
      void refreshCloudLibrary(true);
    });
    await expect.poll(() => readStarted, { timeout: 10_000 }).toBe(true);
    const next = await createPublishedSnapshot(session.pageA, session.hostA, 'After');
    await session.pageB.evaluate(async () => {
      const modulePath = '/src/composables/useCloudLibrary.ts';
      const { refreshCloudLibrary } = await import(modulePath);
      void refreshCloudLibrary(true);
    });
    release();
    await expect(snapshotRow(session.pageB, next)).toBeVisible({ timeout: 10_000 });
    expect(reads).toBe(2);
  } catch (error) {
    failed = true;
    throw error;
  } finally {
    release();
    await session.close(failed);
  }
});

test('background refresh preserves unsaved settings and does not submit configuration', async ({
  browser,
}) => {
  const runRoot = await createRunRoot('refresh-settings');
  const seeded = await seedEmptyCloudWithLocalGame(runRoot);
  const session = await startDualSession(browser, {
    ...seeded,
    runRoot,
    label: 'refresh-settings',
  });
  let failed = false;
  try {
    await createLibrary(session.pageA);
    await session.pageA.getByRole('button', { name: 'Settings', exact: true }).click();
    await session.pageA.getByRole('button', { name: 'Quick Actions', exact: true }).click();
    const save = session.pageA.getByRole('button', { name: 'Save hotkeys', exact: true });
    await expect(save).toBeDisabled();
    const sound = session.pageA
      .getByText('Play sound when quick actions finish', { exact: true })
      .locator('..')
      .getByRole('switch');
    const before = await sound.getAttribute('aria-checked');
    await sound.click();
    await expect(save).toBeEnabled();
    let saves = 0;
    session.pageA.on('request', (request) => {
      if (new URL(request.url()).pathname === '/api/v1/set-config') saves += 1;
    });
    await session.pageA.evaluate(async () => {
      const modulePath = '/src/composables/useCloudLibrary.ts';
      const { refreshCloudLibrary } = await import(modulePath);
      await refreshCloudLibrary();
    });
    await expect(sound).toHaveAttribute('aria-checked', before === 'true' ? 'false' : 'true');
    // Settings autosave is debounced by 500 ms; a read must not schedule that write.
    await session.pageA.waitForTimeout(750);
    expect(saves).toBe(0);
    await expect(save).toBeEnabled();
  } catch (error) {
    failed = true;
    throw error;
  } finally {
    await session.close(failed);
  }
});

test('game order remains a draft while refreshing current game definitions', async ({
  browser,
}) => {
  const runRoot = await createRunRoot('refresh-game-order');
  const seeded = await seedEmptyCloudWithLocalGame(runRoot);
  const session = await startDualSession(browser, {
    ...seeded,
    runRoot,
    label: 'refresh-game-order',
  });
  let failed = false;
  try {
    await addGameViaApi(session.hostA, 'Second game', [
      { type: 'File', path: seeded.deviceA.savePath },
    ]);
    await createLibrary(session.pageA);
    await session.pageA.getByRole('button', { name: 'Settings', exact: true }).click();
    await session.pageA.getByRole('button', { name: 'Game order', exact: true }).click();
    const save = session.pageA.getByRole('button', { name: 'Save default order', exact: true });
    await expect(save).toBeDisabled();
    const rows = session.pageA.locator('[data-game-order-id]');
    await expect(rows).toHaveCount(2);
    const first = (await rows.nth(0).boundingBox())!;
    const second = (await rows.nth(1).boundingBox())!;
    await session.pageA.mouse.move(first.x + 20, first.y + first.height / 2);
    await session.pageA.mouse.down();
    await session.pageA.mouse.move(second.x + 20, second.y + second.height - 2, { steps: 12 });
    await session.pageA.mouse.up();
    await expect(rows).toHaveText(['Second game', GAME_NAME]);
    await expect(save).toBeEnabled();
    await addGameViaApi(session.hostA, 'Added while editing', [
      { type: 'File', path: seeded.deviceA.savePath },
    ]);
    await session.pageA.evaluate(async () => {
      const modulePath = '/src/composables/useCloudLibrary.ts';
      const { refreshCloudLibrary } = await import(modulePath);
      await refreshCloudLibrary();
    });
    await expect(rows).toHaveText(['Second game', GAME_NAME, 'Added while editing']);
    const savedNames = async () => {
      const result = await hostPost<{ games: Array<{ name: string }> }>(
        session.hostA,
        '/api/v1/get-local-config'
      );
      expect(result.ok, result.raw).toBe(true);
      return result.data.games.map((game) => game.name);
    };
    expect(await savedNames()).toEqual([GAME_NAME, 'Second game', 'Added while editing']);
    await save.click();
    await expect(save).toBeDisabled();
    expect(await savedNames()).toEqual(['Second game', GAME_NAME, 'Added while editing']);
  } catch (error) {
    failed = true;
    throw error;
  } finally {
    await session.close(failed);
  }
});
