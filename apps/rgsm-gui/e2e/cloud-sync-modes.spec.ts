import { test, expect } from '@playwright/test';
import { existsSync } from 'node:fs';
import { DEVICE_A_ID, DEVICE_B_ID } from './support/constants';
import {
  cloudArchivePath,
  deviceGameSettings,
  localArchivePath,
  readDeviceProfile,
} from './support/cloud-assertions';
import { readSave, seedEmptyCloudWithLocalGame, writeSave } from './support/cloud-fixture';
import {
  changeGameMode,
  confirmJoinKeepCloud,
  createLibrary,
  createPublishedSnapshot,
  createSnapshotViaApi,
  enableMode,
  latestSnapshotId,
} from './support/gui';
import { createRunRoot } from './support/rgsm-instance';
import { startDualSession } from './support/session';

test('sync modes and enable catch-up', async ({ browser }) => {
  const runRoot = await createRunRoot('modes');
  const seeded = await seedEmptyCloudWithLocalGame(runRoot);
  const session = await startDualSession(browser, { ...seeded, runRoot, label: 'modes' });
  let failed = false;
  try {
    await createLibrary(session.pageA);
    const existing = await createPublishedSnapshot(session.pageA, session.hostA, 'Existing cloud');
    await confirmJoinKeepCloud(session.pageB);

    await changeGameMode(session.pageA, 'Manual');
    expect(
      deviceGameSettings(await readDeviceProfile(seeded.cloudRoot, DEVICE_A_ID)).sync_mode
    ).toBe('manual');
    await writeSave(seeded.deviceA, 'manual-stays-local\n');
    await createSnapshotViaApi(session.hostA, 'Manual local');
    const manualId = await latestSnapshotId(session.hostA, 'Manual local');
    expect(existsSync(cloudArchivePath(seeded.cloudRoot, manualId))).toBe(false);

    await enableMode(session.pageA, session.hostA, 'Cloud Backup', 'Keep in cloud');
    expect(existsSync(localArchivePath(seeded.deviceB.appDataDir, existing))).toBe(false);
    await writeSave(seeded.deviceA, 'cloud-backup-auto\n');
    await createSnapshotViaApi(session.hostA, 'Auto upload');
    const autoId = await latestSnapshotId(session.hostA, 'Auto upload');
    expect(existsSync(cloudArchivePath(seeded.cloudRoot, autoId))).toBe(true);

    const beforeB = await readSave(seeded.deviceB);
    await enableMode(session.pageB, session.hostB, 'Cloud Backup', 'Download to this device');
    expect(existsSync(localArchivePath(seeded.deviceB.appDataDir, existing))).toBe(true);
    expect(await readSave(seeded.deviceB)).toBe(beforeB);

    await enableMode(session.pageB, session.hostB, 'Multi-device Sync', 'Keep in cloud');
    expect(
      deviceGameSettings(await readDeviceProfile(seeded.cloudRoot, DEVICE_B_ID)).sync_mode
    ).toBe('multi_device_sync');
  } catch (error) {
    failed = true;
    throw error;
  } finally {
    await session.close(failed);
  }
});
