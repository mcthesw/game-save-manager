import { test, expect } from '@playwright/test';
import { existsSync } from 'node:fs';
import { cloudArchivePath, localArchivePath } from './support/cloud-assertions';
import { seedEmptyCloudWithLocalGame } from './support/cloud-fixture';
import {
  confirmJoinKeepCloud,
  createLibrary,
  createPublishedSnapshot,
  downloadSnapshot,
  evictCloudCopy,
  evictLocalCopy,
  openGame,
  uploadSnapshot,
} from './support/gui';
import { createRunRoot } from './support/rgsm-instance';
import { startDualSession } from './support/session';

test('evict local or cloud copy without deleting snapshot', async ({ browser }) => {
  const runRoot = await createRunRoot('evict');
  const seeded = await seedEmptyCloudWithLocalGame(runRoot);
  const session = await startDualSession(browser, { ...seeded, runRoot, label: 'evict' });
  let failed = false;
  try {
    await createLibrary(session.pageA);
    const snapshotId = await createPublishedSnapshot(session.pageA, session.hostA, 'Keep record');
    await confirmJoinKeepCloud(session.pageB);
    await openGame(session.pageB);
    await downloadSnapshot(session.pageB, snapshotId);

    await openGame(session.pageA);
    await evictLocalCopy(session.pageA, snapshotId);
    expect(existsSync(localArchivePath(seeded.deviceA.appDataDir, snapshotId))).toBe(false);
    expect(existsSync(cloudArchivePath(seeded.cloudRoot, snapshotId))).toBe(true);

    await downloadSnapshot(session.pageA, snapshotId);
    expect(existsSync(localArchivePath(seeded.deviceA.appDataDir, snapshotId))).toBe(true);

    await evictCloudCopy(session.pageA, snapshotId);
    expect(existsSync(cloudArchivePath(seeded.cloudRoot, snapshotId))).toBe(false);
    expect(existsSync(localArchivePath(seeded.deviceA.appDataDir, snapshotId))).toBe(true);

    await openGame(session.pageB);
    const download = session.pageB
      .getByRole('row')
      .filter({ hasText: snapshotId })
      .getByRole('button', { name: 'Download to this device' });
    if (await download.isVisible().catch(() => false)) {
      await expect(download).toBeDisabled();
    } else {
      await expect(
        session.pageB
          .getByRole('row')
          .filter({ hasText: snapshotId })
          .getByRole('button', { name: 'Upload to cloud' })
      ).toBeVisible();
    }

    await openGame(session.pageA);
    await uploadSnapshot(session.pageA, snapshotId);
    expect(existsSync(cloudArchivePath(seeded.cloudRoot, snapshotId))).toBe(true);
  } catch (error) {
    failed = true;
    throw error;
  } finally {
    await session.close(failed);
  }
});
