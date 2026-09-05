import { test, expect } from '@playwright/test';
import { existsSync } from 'node:fs';
import {
  cloudArchivePath,
  cloudPaths,
  expectSharedRetentionLimit,
  liveSnapshotState,
  readJson,
} from './support/cloud-assertions';
import { seedEmptyCloudWithLocalGame, seedLocalAutomaticSnapshots } from './support/cloud-fixture';
import {
  createLibrary,
  openGame,
  protectSnapshot,
  setSharedRetention,
  uploadSnapshot,
} from './support/gui';
import { createRunRoot, hostPost } from './support/rgsm-instance';
import { GAME_NAME } from './support/constants';
import { startDualSession } from './support/session';

test('protect snapshot and set shared retention limit', async ({ browser }) => {
  const runRoot = await createRunRoot('protect');
  const seeded = await seedEmptyCloudWithLocalGame(runRoot);
  const [keepId, second, third, fourth] = await seedLocalAutomaticSnapshots(seeded.deviceA, [
    'Keep this',
    'Second',
    'Third',
    'Fourth',
  ]);
  const session = await startDualSession(browser, { ...seeded, runRoot, label: 'protect' });
  let failed = false;
  try {
    await createLibrary(session.pageA);
    const provenanceChange = await hostPost(session.hostA, '/api/v1/set-snapshot-created-by', {
      gameName: GAME_NAME,
      snapshotDate: keepId,
      createdBy: 'Manual',
    });
    expect(provenanceChange.ok).toBe(false);
    await openGame(session.pageA);
    for (const snapshotId of [keepId, second, third, fourth]) {
      await uploadSnapshot(session.pageA, snapshotId!);
    }
    await protectSnapshot(session.pageA, keepId!);
    await setSharedRetention(session.pageA, 1);

    expectSharedRetentionLimit(await readJson(cloudPaths(seeded.cloudRoot).sharedLibrary), 1);
    const manifest = await readJson(cloudPaths(seeded.cloudRoot).manifest);
    expect(liveSnapshotState(manifest, keepId).retention_protected).toBe(true);
    expect(existsSync(cloudArchivePath(seeded.cloudRoot, keepId!))).toBe(true);
    expect(existsSync(cloudArchivePath(seeded.cloudRoot, third!))).toBe(true);
    expect(existsSync(cloudArchivePath(seeded.cloudRoot, fourth!))).toBe(true);
    expect(existsSync(cloudArchivePath(seeded.cloudRoot, second!))).toBe(false);
  } catch (error) {
    failed = true;
    throw error;
  } finally {
    await session.close(failed);
  }
});
