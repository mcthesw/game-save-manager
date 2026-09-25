import { test, expect } from '@playwright/test';
import { existsSync } from 'node:fs';
import { readFile, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { STORAGE_KEY } from './support/constants';
import {
  cloudArchivePath,
  cloudPaths,
  expectSharedLibraryHasGame,
  readJson,
} from './support/cloud-assertions';
import { seedEmptyCloudWithLocalGame } from './support/cloud-fixture';
import {
  connectLibrary,
  createLibrary,
  createPublishedSnapshot,
  permanentlyDeleteGame,
} from './support/gui';
import { createRunRoot, hostPost } from './support/rgsm-instance';
import { startDualSession } from './support/session';

test('create library then permanently delete game', async ({ browser }) => {
  const runRoot = await createRunRoot('delete-game');
  const seeded = await seedEmptyCloudWithLocalGame(runRoot);
  const session = await startDualSession(browser, { ...seeded, runRoot, label: 'delete-game' });
  let failed = false;
  try {
    await createLibrary(session.pageA);
    const snapshotId = await createPublishedSnapshot(session.pageA, session.hostA, 'To delete');
    await connectLibrary(session.pageB);
    expectSharedLibraryHasGame(await readJson(cloudPaths(seeded.cloudRoot).sharedLibrary));

    await permanentlyDeleteGame(session.pageA);
    const library = await readJson(cloudPaths(seeded.cloudRoot).sharedLibrary);
    const games = library.games as Array<{ storage_key?: string }>;
    expect(games.some((game) => game.storage_key === 'Echo Keep')).toBe(false);
    expect(existsSync(cloudArchivePath(seeded.cloudRoot, snapshotId))).toBe(false);
    expect(existsSync(join(seeded.cloudRoot, 'v2', 'archives', STORAGE_KEY))).toBe(false);
  } catch (error) {
    failed = true;
    throw error;
  } finally {
    await session.close(failed);
  }
});

test('shared game can be deleted locally from either page without rejoining on refresh', async ({
  browser,
}, testInfo) => {
  const runRoot = await createRunRoot('delete-shared-locally');
  const seeded = await seedEmptyCloudWithLocalGame(runRoot);
  const session = await startDualSession(browser, {
    ...seeded,
    runRoot,
    label: 'delete-shared-locally',
  });
  let failed = false;
  try {
    const page = session.pageA;
    await createLibrary(page);
    const snapshotId = await createPublishedSnapshot(page, session.hostA, 'Keep in cloud');
    await connectLibrary(session.pageB);
    const liveSave = await readFile(seeded.deviceA.savePath);
    await page.goto('/Management/Echo%20Keep?gameId=Echo%20Keep');
    await page.getByRole('button', { name: 'More actions' }).click();
    await expect(
      page.getByRole('menuitem', { name: 'Delete from cloud and all devices' })
    ).toBeVisible();
    await page.getByRole('menuitem', { name: 'Delete from this device' }).click();
    const localDialog = page.getByRole('dialog', { name: 'Delete from this device' });
    await localDialog.getByRole('textbox').fill('yes');
    await localDialog.getByRole('button', { name: 'Delete from this device' }).click();
    await expect(page).toHaveURL(/\/$/);
    expect(existsSync(join(seeded.deviceA.archiveRoot, STORAGE_KEY))).toBe(false);
    expect(existsSync(cloudArchivePath(seeded.cloudRoot, snapshotId))).toBe(true);
    expect(await readFile(seeded.deviceA.savePath)).toEqual(liveSave);

    // Another device changes the shared definition while this one is unmanaged.
    const sharedPath = cloudPaths(seeded.cloudRoot).sharedLibrary;
    const library = await readJson(sharedPath);
    (library.games as Array<{ name: string }>)[0]!.name = 'Renamed in cloud';
    await writeFile(sharedPath, JSON.stringify(library));
    const refreshed = await hostPost(session.hostA, '/api/v1/refresh-cloud-archive-library');
    expect(refreshed.ok, refreshed.raw).toBe(true);
    const statuses = await hostPost<Array<{ game_id: string; managed: boolean }>>(
      session.hostA,
      '/api/v1/get-current-device-game-statuses'
    );
    expect(statuses.data.find((game) => game.game_id === STORAGE_KEY)?.managed).toBe(false);
    const other = await hostPost<Array<{ game_id: string; managed: boolean }>>(
      session.hostB,
      '/api/v1/get-current-device-game-statuses'
    );
    expect(other.data.find((game) => game.game_id === STORAGE_KEY)?.managed).toBe(true);
    expect(existsSync(join(seeded.deviceA.archiveRoot, STORAGE_KEY))).toBe(false);

    await page.goto('/SyncSettings');
    await page.getByRole('button', { name: 'Manage on this device' }).click();
    await expect(page.getByRole('button', { name: 'Manage on this device' })).toHaveCount(0);
    await page.getByRole('button', { name: 'Delete game', exact: true }).click();
    await page.screenshot({
      path: testInfo.outputPath('acceptance-cloud-delete-menu.png'),
      animations: 'disabled',
    });
    await page.getByRole('menuitem', { name: 'Delete from this device' }).click();
    await localDialog.getByRole('textbox').fill('yes');
    await localDialog.getByRole('button', { name: 'Delete from this device' }).click();
    await expect(page.getByRole('button', { name: 'Manage on this device' })).toBeVisible();
    expect(existsSync(cloudArchivePath(seeded.cloudRoot, snapshotId))).toBe(true);

    await page.goto('/Management/Renamed%20in%20cloud?gameId=Echo%20Keep');
    await page.getByRole('button', { name: 'Keep Empty' }).click();
    await page.getByRole('button', { name: 'More actions' }).click();
    await expect(page.getByRole('menuitem', { name: 'Delete from this device' })).toHaveCount(0);
    await page.getByRole('menuitem', { name: 'Delete from cloud and all devices' }).click();
    const globalDialog = page.getByRole('dialog', { name: 'Delete from cloud and all devices' });
    await globalDialog.getByRole('textbox').fill('YES');
    await globalDialog.getByRole('button', { name: 'Delete from cloud and all devices' }).click();
    await expect(globalDialog.getByText('Enter yes to confirm deletion')).toBeVisible();
    await globalDialog.getByRole('textbox').fill('yes');
    await globalDialog.getByRole('button', { name: 'Delete from cloud and all devices' }).click();
    await expect(page).toHaveURL(/\/$/);
    expect(existsSync(cloudArchivePath(seeded.cloudRoot, snapshotId))).toBe(false);
    expect(await readFile(seeded.deviceA.savePath)).toEqual(liveSave);
  } catch (error) {
    failed = true;
    throw error;
  } finally {
    await session.close(failed);
  }
});
