import { test, expect } from '@playwright/test';
import { existsSync } from 'node:fs';
import { basename, join } from 'node:path';
import { DEVICE_A_ID, DEVICE_B_ID, STORAGE_KEY } from './support/constants';
import {
  localArchivePath,
  cloudPaths,
  expectDeviceHead,
  readJson,
} from './support/cloud-assertions';
import { readSave, seedEmptyCloudWithLocalGame, writeSave } from './support/cloud-fixture';
import {
  connectLibrary,
  createLibrary,
  createPublishedSnapshot,
  enableMode,
  createSnapshotViaApi,
  latestSnapshotId,
  reviewProgress,
  changeGameMode,
} from './support/gui';
import { createRunRoot } from './support/rgsm-instance';
import { startDualSession } from './support/session';
import { pendingProgress, deferPendingProgress } from './support/progress';

for (const mode of ['Manual', 'Multi-device Sync'] as const) {
  test(`keep local instead of taking the other device save in ${mode}`, async ({ browser }) => {
    const runRoot = await createRunRoot('keep-local');
    const seeded = await seedEmptyCloudWithLocalGame(runRoot);
    const session = await startDualSession(browser, { ...seeded, runRoot, label: 'keep-local' });
    let failed = false;
    try {
      await createLibrary(session.pageA);
      await createPublishedSnapshot(session.pageA, session.hostA, 'Shared parent');
      await enableMode(session.pageA, session.hostA, 'Multi-device Sync', 'Keep in cloud');
      await connectLibrary(session.pageB);
      await enableMode(
        session.pageB,
        session.hostB,
        'Multi-device Sync',
        'Download to this device'
      );
      if (mode === 'Manual') {
        await deferPendingProgress(session.pageB);
        await changeGameMode(session.pageA, 'Manual');
        await changeGameMode(session.pageB, 'Manual');
      }

      await writeSave(seeded.deviceA, 'branch-a\n');
      await writeSave(seeded.deviceB, 'branch-b\n');
      await createSnapshotViaApi(session.hostA, 'A branch');
      await createSnapshotViaApi(session.hostB, 'B branch');
      const aBranch = await latestSnapshotId(session.hostA, 'A branch');
      const bBranch = await latestSnapshotId(session.hostB, 'B branch');
      // Uploads preserve the local archive name, including unpublished snapshots.
      const cloudDirectory = join(seeded.cloudRoot, 'v2', 'archives', STORAGE_KEY);
      const aLocalCopy = localArchivePath(seeded.deviceA.appDataDir, aBranch);
      const bLocalCopy = localArchivePath(seeded.deviceB.appDataDir, bBranch);
      expect(existsSync(aLocalCopy)).toBe(true);
      expect(existsSync(bLocalCopy)).toBe(true);
      const aCloudCopy = join(cloudDirectory, basename(aLocalCopy));
      const bCloudCopy = join(cloudDirectory, basename(bLocalCopy));
      if (mode === 'Manual') {
        expect(existsSync(aCloudCopy)).toBe(false);
      }

      await expect
        .poll(async () => (await reviewProgress(session.hostA)).requires_choice)
        .toBe(true);
      const prompt = session.pageA.getByRole('dialog', {
        name: 'Progress available from other devices',
      });
      if (mode === 'Manual') {
        await session.pageA.reload();
        await session.pageA
          .getByRole('button', { name: /^(Compare|Progress diverged, compare)$/ })
          .click();
      } else {
        await (await pendingProgress(session.pageA))
          .getByRole('button', { name: 'Compare', exact: true })
          .click();
      }
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
      ).toContainText(mode);
      await expect(prompt).toBeHidden();
      await expect(async () => {
        const manifest = await readJson(cloudPaths(seeded.cloudRoot).manifest);
        expectDeviceHead(manifest, DEVICE_A_ID, aBranch);
        expectDeviceHead(manifest, DEVICE_B_ID, bBranch);
      }).toPass({ timeout: 20_000 });
      expect(await readSave(seeded.deviceA)).toBe('branch-a\n');
      expect(await readSave(seeded.deviceB)).toBe('branch-b\n');
      if (mode === 'Manual') {
        expect(existsSync(aCloudCopy)).toBe(false);
        expect(existsSync(bCloudCopy)).toBe(false);
      }
    } catch (error) {
      failed = true;
      throw error;
    } finally {
      await session.close(failed);
    }
  });
}
