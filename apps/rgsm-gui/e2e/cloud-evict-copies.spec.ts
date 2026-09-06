import { test, expect } from '@playwright/test';
import { existsSync } from 'node:fs';
import { cloudArchivePath, localArchivePath } from './support/cloud-assertions';
import { readSave, seedEmptyCloudWithLocalGame } from './support/cloud-fixture';
import {
  connectLibrary,
  createLibrary,
  createPublishedSnapshot,
  downloadSnapshot,
  enableMode,
  evictCloudCopy,
  evictLocalCopy,
  openGame,
  snapshotRow,
  uploadSnapshot,
} from './support/gui';
import { createSnapshotForGame } from './support/local-gui';
import { GAME_NAME } from './support/constants';
import { createRunRoot } from './support/rgsm-instance';
import { startDualSession } from './support/session';

test('evict local or cloud copy without deleting snapshot', async ({ browser }) => {
  const runRoot = await createRunRoot('evict');
  const seeded = await seedEmptyCloudWithLocalGame(runRoot);
  const session = await startDualSession(browser, { ...seeded, runRoot, label: 'evict' });
  let failed = false;
  try {
    await createLibrary(session.pageA);
    await enableMode(session.pageA, session.hostA, 'Cloud Backup', 'Keep in cloud');
    const snapshotId = await createPublishedSnapshot(session.pageA, session.hostA, 'Keep record');
    const saves = [await readSave(seeded.deviceA), await readSave(seeded.deviceB)];
    await connectLibrary(session.pageB);
    await openGame(session.pageB);
    await downloadSnapshot(session.pageB, snapshotId);

    await openGame(session.pageA);
    await snapshotRow(session.pageA, snapshotId)
      .getByRole('button', { name: 'Remove from this device' })
      .click();
    const localDialog = session.pageA.getByRole('dialog');
    await expect(localDialog).toContainText('Keep record');
    await expect(localDialog).not.toContainText(snapshotId);
    await expect(localDialog).not.toContainText('automatic Sync Mode');
    await localDialog.getByRole('button', { name: 'Remove local copy' }).click();
    await expect
      .poll(() => existsSync(localArchivePath(seeded.deviceA.appDataDir, snapshotId)))
      .toBe(false);
    expect(existsSync(localArchivePath(seeded.deviceA.appDataDir, snapshotId))).toBe(false);
    expect(existsSync(cloudArchivePath(seeded.cloudRoot, snapshotId))).toBe(true);

    // Disk removal precedes the page refresh; wait before using the idempotent download helper.
    await expect(
      snapshotRow(session.pageA, snapshotId).getByRole('button', {
        name: 'Download to this device',
      })
    ).toBeVisible();
    await downloadSnapshot(session.pageA, snapshotId);
    expect(existsSync(localArchivePath(seeded.deviceA.appDataDir, snapshotId))).toBe(true);
    expect(existsSync(localArchivePath(seeded.deviceB.appDataDir, snapshotId))).toBe(true);

    await evictCloudCopy(session.pageA, snapshotId);
    expect(existsSync(cloudArchivePath(seeded.cloudRoot, snapshotId))).toBe(false);
    expect(existsSync(localArchivePath(seeded.deviceA.appDataDir, snapshotId))).toBe(true);

    // A new automatic upload must not recreate an explicitly removed cloud copy.
    const next = await createSnapshotForGame(session.hostA, GAME_NAME, 'New automatic upload');
    await expect.poll(() => existsSync(cloudArchivePath(seeded.cloudRoot, next))).toBe(true);
    expect(existsSync(cloudArchivePath(seeded.cloudRoot, snapshotId))).toBe(false);

    await openGame(session.pageB);
    const download = snapshotRow(session.pageB, snapshotId).getByRole('button', {
      name: 'Download to this device',
    });
    if (await download.isVisible().catch(() => false)) {
      await expect(download).toBeDisabled();
    } else {
      await expect(
        snapshotRow(session.pageB, snapshotId).getByRole('button', { name: 'Upload to cloud' })
      ).toBeVisible();
    }

    await openGame(session.pageA);
    await uploadSnapshot(session.pageA, snapshotId);
    expect(existsSync(cloudArchivePath(seeded.cloudRoot, snapshotId))).toBe(true);
    expect([await readSave(seeded.deviceA), await readSave(seeded.deviceB)]).toEqual(saves);
  } catch (error) {
    failed = true;
    throw error;
  } finally {
    await session.close(failed);
  }
});

test('removing the last known copy warns without deleting history or live saves', async ({
  browser,
}) => {
  const runRoot = await createRunRoot('last-copy');
  const seeded = await seedEmptyCloudWithLocalGame(runRoot);
  const session = await startDualSession(browser, { ...seeded, runRoot, label: 'last-copy' });
  let failed = false;
  try {
    await createLibrary(session.pageA);
    const id = await createPublishedSnapshot(session.pageA, session.hostA, 'Last copy');
    const save = await readSave(seeded.deviceA);
    await openGame(session.pageA);
    await evictLocalCopy(session.pageA, id);
    await snapshotRow(session.pageA, id).getByRole('button', { name: 'Remove cloud copy' }).click();
    const dialog = session.pageA.getByRole('dialog');
    await expect(dialog).toContainText('last known copy');
    await expect(dialog).toContainText('Last copy');
    await dialog.getByRole('button', { name: 'Cancel' }).click();
    expect(existsSync(cloudArchivePath(seeded.cloudRoot, id))).toBe(true);
    await snapshotRow(session.pageA, id).getByRole('button', { name: 'Remove cloud copy' }).click();
    await dialog.getByRole('button', { name: 'Remove cloud copy' }).click();
    await expect.poll(() => existsSync(cloudArchivePath(seeded.cloudRoot, id))).toBe(false);
    await expect(snapshotRow(session.pageA, id)).toBeVisible();
    await expect(
      snapshotRow(session.pageA, id).getByRole('button', { name: 'Apply', exact: true })
    ).toBeDisabled();
    expect(await readSave(seeded.deviceA)).toBe(save);
  } catch (error) {
    failed = true;
    throw error;
  } finally {
    await session.close(failed);
  }
});
