import { test, expect } from '@playwright/test';
import { readFile, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { cloudPaths, readJson } from './support/cloud-assertions';
import { seedEmptyCloudWithLocalGame } from './support/cloud-fixture';
import { GAME_NAME, STORAGE_KEY } from './support/constants';
import { createLibrary, openGame } from './support/gui';
import {
  addGameViaApi,
  createSnapshotForGame,
  getLocalGame,
  updateGameViaApi,
} from './support/local-gui';
import { createRunRoot, hostPost, startRgsmHost, type RgsmHost } from './support/rgsm-instance';
import { startDualSession } from './support/session';

test('offline game edits and additions survive restart and publish without joining each game', async ({
  browser,
}) => {
  const runRoot = await createRunRoot('offline-metadata');
  const seeded = await seedEmptyCloudWithLocalGame(runRoot);
  const session = await startDualSession(browser, {
    ...seeded,
    runRoot,
    label: 'offline-metadata',
  });
  let restarted: RgsmHost | undefined;
  let failed = false;
  const sharedPath = cloudPaths(seeded.cloudRoot).sharedLibrary;
  let original: Buffer | undefined;
  try {
    await createLibrary(session.pageA);
    await openGame(session.pageA);
    original = await readFile(sharedPath);
    await writeFile(sharedPath, 'offline test');
    await session.pageA.getByRole('button', { name: 'View managed files' }).click();
    const drawer = session.pageA.getByRole('dialog');
    await drawer.getByRole('textbox', { name: 'Game name', exact: true }).fill('Offline title');
    const saved = session.pageA.waitForResponse((response) =>
      response.url().endsWith('/api/v1/update-game')
    );
    await drawer.getByRole('button', { name: 'save', exact: true }).click();
    expect((await saved).ok()).toBe(true);
    await expect(drawer).toBeHidden();
    await expect(session.pageA.getByRole('heading', { name: 'Offline title' })).toBeVisible();
    await expect(
      session.pageA.getByText('Game details pending sync', { exact: true })
    ).toBeVisible();
    await addGameViaApi(session.hostA, 'Added offline', [
      { type: 'File', path: seeded.deviceA.savePath },
    ]);
    await createSnapshotForGame(session.hostA, 'Offline title', 'Still available offline');
    await session.contextA.close();
    await session.hostA.stop();
    restarted = await startRgsmHost({
      appDataDir: seeded.deviceA.appDataDir,
      deviceId: seeded.deviceA.id,
      logPath: join(runRoot, 'logs', 'restarted.log'),
    });
    expect((await getLocalGame(restarted, 'Offline title')).storage_key).toBe(STORAGE_KEY);
    await getLocalGame(restarted, 'Added offline');
    await writeFile(sharedPath, original);
    original = undefined;
    const refreshed = await hostPost(restarted, '/api/v1/refresh-cloud-archive-library');
    expect(refreshed.ok, refreshed.raw).toBe(true);
    const remote = await readJson(sharedPath);
    expect(remote.games).toEqual(
      expect.arrayContaining([
        expect.objectContaining({ name: 'Offline title', storage_key: STORAGE_KEY }),
        expect.objectContaining({ name: 'Added offline' }),
      ])
    );
    const status = await hostPost<Array<{ metadata_sync_pending: boolean }>>(
      restarted,
      '/api/v1/get-current-device-game-statuses'
    );
    expect(status.data.every((game) => !game.metadata_sync_pending)).toBe(true);
  } catch (error) {
    failed = true;
    throw error;
  } finally {
    if (original) await writeFile(sharedPath, original);
    await restarted?.stop();
    await session.close(failed);
  }
});

test('offline metadata conflict uses the existing definition choice and keeps local backup usable', async ({
  browser,
}, testInfo) => {
  const runRoot = await createRunRoot('offline-metadata-choice');
  const seeded = await seedEmptyCloudWithLocalGame(runRoot);
  const session = await startDualSession(browser, {
    ...seeded,
    runRoot,
    label: 'offline-metadata-choice',
  });
  let failed = false;
  const sharedPath = cloudPaths(seeded.cloudRoot).sharedLibrary;
  try {
    await createLibrary(session.pageA);
    const original = await readJson(sharedPath);
    await writeFile(sharedPath, 'offline test');
    const game = await getLocalGame(session.hostA, GAME_NAME);
    await updateGameViaApi(session.hostA, STORAGE_KEY, { ...game, name: 'My offline title' });
    const games = original.games as Array<{ storage_key: string; name: string }>;
    games.find((game) => game.storage_key === STORAGE_KEY)!.name = 'Title from another device';
    await writeFile(sharedPath, JSON.stringify(original));
    const refresh = await hostPost(session.hostA, '/api/v1/refresh-cloud-archive-library');
    expect(refresh.ok, refresh.raw).toBe(true);
    await createSnapshotForGame(session.hostA, 'My offline title', 'During a metadata conflict');
    await openGame(session.pageA, 'My offline title');
    await session.pageA.getByRole('button', { name: 'Choose game definition' }).click();
    const dialog = session.pageA.getByRole('dialog', {
      name: 'Choose game definition',
      exact: true,
    });
    await expect(dialog.getByText('My offline title', { exact: true }).first()).toBeVisible();
    await expect(
      dialog.getByText('Title from another device', { exact: true }).first()
    ).toBeVisible();
    await session.pageA.setViewportSize({ width: 1440, height: 1000 });
    await session.pageA.screenshot({
      path: testInfo.outputPath('offline-metadata-conflict.png'),
      animations: 'disabled',
    });
    await dialog.getByRole('button', { name: 'Keep the cloud version', exact: true }).click();
    await dialog.getByRole('button', { name: 'Use selected definition', exact: true }).click();
    await expect(dialog).toBeHidden();
    await getLocalGame(session.hostA, 'Title from another device');
    await expect(session.pageA.getByRole('button', { name: 'Choose game definition' })).toHaveCount(
      0
    );
  } catch (error) {
    failed = true;
    throw error;
  } finally {
    await session.close(failed);
  }
});
