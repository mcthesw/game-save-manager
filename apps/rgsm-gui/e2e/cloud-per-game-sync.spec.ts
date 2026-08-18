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
  confirmJoinKeepCloud,
  createLibrary,
  createPublishedSnapshot,
  createSnapshotViaApi,
  downloadSnapshot,
  enableMode,
  latestSnapshotId,
  openGame,
  toggleCloudEnabled,
  uploadSnapshot,
} from './support/gui';
import { createRunRoot } from './support/rgsm-instance';
import { startDualSession } from './support/session';

test('per-game upload download disable re-enable', async ({ browser }) => {
  const runRoot = await createRunRoot('per-game');
  const seeded = await seedEmptyCloudWithLocalGame(runRoot);
  const session = await startDualSession(browser, { ...seeded, runRoot, label: 'per-game' });
  let failed = false;
  try {
    await createLibrary(session.pageA);
    const baseId = await createPublishedSnapshot(session.pageA, session.hostA, 'Shared base');
    await enableMode(session.pageA, session.hostA, 'Cloud Backup', 'Keep in cloud');
    await confirmJoinKeepCloud(session.pageB);
    await enableMode(session.pageB, session.hostB, 'Cloud Backup', 'Keep in cloud');
    const saveB = await readSave(seeded.deviceB);
    await openGame(session.pageB);
    await downloadSnapshot(session.pageB, baseId);
    expect(existsSync(localArchivePath(seeded.deviceB.appDataDir, baseId))).toBe(true);
    expect(await readSave(seeded.deviceB)).toBe(saveB);

    await writeSave(seeded.deviceA, 'a-only-upload\n');
    const aOnly = await createPublishedSnapshot(session.pageA, session.hostA, 'A only');
    expect(existsSync(cloudArchivePath(seeded.cloudRoot, aOnly))).toBe(true);
    expect(existsSync(localArchivePath(seeded.deviceB.appDataDir, aOnly))).toBe(false);

    const saveBAfterA = await readSave(seeded.deviceB);
    await openGame(session.pageB);
    await downloadSnapshot(session.pageB, aOnly);
    expect(existsSync(localArchivePath(seeded.deviceB.appDataDir, aOnly))).toBe(true);
    expect(existsSync(cloudArchivePath(seeded.cloudRoot, aOnly))).toBe(true);
    expect(await readSave(seeded.deviceB)).toBe(saveBAfterA);

    await toggleCloudEnabled(session.pageA, session.hostA, false);
    expect(
      deviceGameSettings(await readDeviceProfile(seeded.cloudRoot, DEVICE_A_ID))
    ).toMatchObject({
      cloud_sync_enabled: false,
      sync_mode: 'cloud_backup',
    });
    expect(
      deviceGameSettings(await readDeviceProfile(seeded.cloudRoot, DEVICE_B_ID)).sync_mode
    ).toBe('cloud_backup');

    await writeSave(seeded.deviceA, 'disabled-should-stay-local\n');
    await createSnapshotViaApi(session.hostA, 'While disabled');
    const disabledId = await latestSnapshotId(session.hostA, 'While disabled');
    expect(existsSync(cloudArchivePath(seeded.cloudRoot, disabledId))).toBe(false);

    await toggleCloudEnabled(session.pageA, session.hostA, true);
    await openGame(session.pageA);
    await uploadSnapshot(session.pageA, disabledId);
    expect(existsSync(cloudArchivePath(seeded.cloudRoot, disabledId))).toBe(true);
  } catch (error) {
    failed = true;
    throw error;
  } finally {
    await session.close(failed);
  }
});
