import { test, expect } from '@playwright/test';
import { DEVICE_A_ID, DEVICE_B_ID } from './support/constants';
import { cloudPaths, expectDeviceHead, readJson } from './support/cloud-assertions';
import { readSave, seedEmptyCloudWithLocalGame, writeSave } from './support/cloud-fixture';
import {
  confirmJoinKeepCloud,
  createLibrary,
  createPublishedSnapshot,
  enableMode,
  keepLocalProgress,
  reviewProgress,
} from './support/gui';
import { createRunRoot } from './support/rgsm-instance';
import { startDualSession } from './support/session';

test('keep local instead of taking the other device save', async ({ browser }) => {
  const runRoot = await createRunRoot('keep-local');
  const seeded = await seedEmptyCloudWithLocalGame(runRoot);
  const session = await startDualSession(browser, { ...seeded, runRoot, label: 'keep-local' });
  let failed = false;
  try {
    await createLibrary(session.pageA);
    await createPublishedSnapshot(session.pageA, session.hostA, 'Shared parent');
    await enableMode(session.pageA, session.hostA, 'Multi-device Sync', 'Keep in cloud');
    await confirmJoinKeepCloud(session.pageB);
    await enableMode(session.pageB, session.hostB, 'Multi-device Sync', 'Download to this device');

    await writeSave(seeded.deviceA, 'branch-a\n');
    await writeSave(seeded.deviceB, 'branch-b\n');
    const aBranch = await createPublishedSnapshot(session.pageA, session.hostA, 'A branch');
    const bBranch = await createPublishedSnapshot(session.pageB, session.hostB, 'B branch');

    const review = await reviewProgress(session.hostA);
    expect(review.requires_choice).toBe(true);
    await keepLocalProgress(session.pageA);
    const manifest = await readJson(cloudPaths(seeded.cloudRoot).manifest);
    expectDeviceHead(manifest, DEVICE_A_ID, aBranch);
    expectDeviceHead(manifest, DEVICE_B_ID, bBranch);
    expect(await readSave(seeded.deviceA)).toBe('branch-a\n');
    expect(await readSave(seeded.deviceB)).toBe('branch-b\n');
  } catch (error) {
    failed = true;
    throw error;
  } finally {
    await session.close(failed);
  }
});
