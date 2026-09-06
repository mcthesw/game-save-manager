import { test, expect } from '@playwright/test';
import { DEVICE_A_ID, DEVICE_B_ID } from './support/constants';
import { cloudPaths, expectDeviceHead, readJson } from './support/cloud-assertions';
import { readSave, seedEmptyCloudWithLocalGame, writeSave } from './support/cloud-fixture';
import {
  connectLibrary,
  createLibrary,
  createPublishedSnapshot,
  enableMode,
  createSnapshotViaApi,
  latestSnapshotId,
  reviewProgress,
} from './support/gui';
import { createRunRoot } from './support/rgsm-instance';
import { startDualSession } from './support/session';
import { pendingProgress } from './support/progress';

test('keep local instead of taking the other device save', async ({ browser }) => {
  const runRoot = await createRunRoot('keep-local');
  const seeded = await seedEmptyCloudWithLocalGame(runRoot);
  const session = await startDualSession(browser, { ...seeded, runRoot, label: 'keep-local' });
  let failed = false;
  try {
    await createLibrary(session.pageA);
    await createPublishedSnapshot(session.pageA, session.hostA, 'Shared parent');
    await enableMode(session.pageA, session.hostA, 'Multi-device Sync', 'Keep in cloud');
    await connectLibrary(session.pageB);
    await enableMode(session.pageB, session.hostB, 'Multi-device Sync', 'Download to this device');

    await writeSave(seeded.deviceA, 'branch-a\n');
    await writeSave(seeded.deviceB, 'branch-b\n');
    await createSnapshotViaApi(session.hostA, 'A branch');
    await createSnapshotViaApi(session.hostB, 'B branch');
    const aBranch = await latestSnapshotId(session.hostA, 'A branch');
    const bBranch = await latestSnapshotId(session.hostB, 'B branch');

    const review = await reviewProgress(session.hostA);
    expect(review.requires_choice).toBe(true);
    const prompt = await pendingProgress(session.pageA);
    await prompt.getByRole('button', { name: 'Compare', exact: true }).click();
    const comparison = session.pageA.getByRole('dialog', { name: /Compare progress/ });
    await comparison.getByRole('button', { name: 'Keep this device' }).click();
    const confirmation = session.pageA.getByRole('dialog', {
      name: "Keep this device's progress?",
    });
    await confirmation.getByRole('button', { name: 'Keep this device' }).click();
    await expect(comparison).toBeHidden();
    await session.pageA.reload();
    await expect(
      session.pageA.getByRole('button', { name: 'Sync mode', includeHidden: true })
    ).toContainText('Multi-device Sync');
    await expect(prompt).toBeHidden();
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
