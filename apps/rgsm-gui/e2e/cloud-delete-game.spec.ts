import { test, expect } from '@playwright/test';
import { existsSync } from 'node:fs';
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
  confirmJoinKeepCloud,
  createLibrary,
  createPublishedSnapshot,
  permanentlyDeleteGame,
} from './support/gui';
import { createRunRoot } from './support/rgsm-instance';
import { startDualSession } from './support/session';

test('create library then permanently delete game', async ({ browser }) => {
  const runRoot = await createRunRoot('delete-game');
  const seeded = await seedEmptyCloudWithLocalGame(runRoot);
  const session = await startDualSession(browser, { ...seeded, runRoot, label: 'delete-game' });
  let failed = false;
  try {
    await createLibrary(session.pageA);
    const snapshotId = await createPublishedSnapshot(session.pageA, session.hostA, 'To delete');
    await confirmJoinKeepCloud(session.pageB);
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
