import { test, expect } from '@playwright/test';
import { existsSync } from 'node:fs';
import { GAME_NAME } from './support/constants';
import {
  cloudPaths,
  expectSharedLibraryHasGame,
  localArchivePath,
  readJson,
} from './support/cloud-assertions';
import { seedEmptyCloudWithLocalGame } from './support/cloud-fixture';
import {
  connectLibrary,
  createLibrary,
  createPublishedSnapshot,
  downloadSnapshot,
  expectLibraryKind,
  openGame,
  openSyncSettings,
} from './support/gui';
import { addGameViaApi } from './support/local-gui';
import { createRunRoot, hostPost } from './support/rgsm-instance';
import { startDualSession } from './support/session';

test('game added after empty V2 creation is published for the second device', async ({
  browser,
}) => {
  const runRoot = await createRunRoot('late-game');
  const seeded = await seedEmptyCloudWithLocalGame(runRoot, {
    gameOnA: false,
    gameOnB: false,
  });
  const session = await startDualSession(browser, { ...seeded, runRoot, label: 'late-game' });
  let failed = false;
  try {
    await createLibrary(session.pageA);
    expect((await readJson(cloudPaths(seeded.cloudRoot).sharedLibrary)).games).toEqual([]);
    await session.pageB.reload();
    await openSyncSettings(session.pageB);
    await expectLibraryKind(session.pageB, 'active');
    await connectLibrary(session.pageB);

    await addGameViaApi(session.hostA, GAME_NAME, [
      { type: 'File', path: seeded.deviceA.savePath },
    ]);

    await expect
      .poll(async () => {
        const library = await readJson(cloudPaths(seeded.cloudRoot).sharedLibrary);
        return (library.games as Array<{ storage_key: string }>).map((game) => game.storage_key);
      })
      .toContain(GAME_NAME);
    expectSharedLibraryHasGame(await readJson(cloudPaths(seeded.cloudRoot).sharedLibrary));

    await session.pageB.reload();
    await openSyncSettings(session.pageB);
    await expect
      .poll(async () => {
        const config = await hostPost<{ games: Array<{ name: string }> }>(
          session.hostB,
          '/api/v1/get-local-config'
        );
        return config.data.games.map((game) => game.name);
      })
      .toContain(GAME_NAME);

    await session.pageA.reload();
    await openGame(session.pageA);
    const snapshotId = await createPublishedSnapshot(
      session.pageA,
      session.hostA,
      'Published after empty library creation'
    );
    await openGame(session.pageB);
    await downloadSnapshot(session.pageB, snapshotId);
    expect(existsSync(localArchivePath(seeded.deviceB.appDataDir, snapshotId))).toBe(true);
  } catch (error) {
    failed = true;
    throw error;
  } finally {
    await session.close(failed);
  }
});
