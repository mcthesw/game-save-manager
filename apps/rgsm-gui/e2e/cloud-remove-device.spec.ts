import { test, expect } from '@playwright/test';
import { existsSync } from 'node:fs';
import { readFile } from 'node:fs/promises';
import { DEVICE_A_ID, DEVICE_B_ID } from './support/constants';
import {
  cloudArchivePath,
  cloudPaths,
  expectDeviceHasNoHead,
  expectDeviceProfiles,
  localArchivePath,
  readJson,
} from './support/cloud-assertions';
import { seedEmptyCloudWithLocalGame, readSave } from './support/cloud-fixture';
import {
  connectLibrary,
  createLibrary,
  createPublishedSnapshot,
  enableMode,
  downloadSnapshot,
  openGame,
  reconnectCloudLibrary,
  removeLibraryDevice,
} from './support/gui';
import { createRunRoot } from './support/rgsm-instance';
import { startDualSession } from './support/session';

test('a removed device reconnects only after confirmation and keeps existing backups', async ({
  browser,
}) => {
  const runRoot = await createRunRoot('remove-device');
  const seeded = await seedEmptyCloudWithLocalGame(runRoot);
  const session = await startDualSession(browser, { ...seeded, runRoot, label: 'remove-device' });
  let failed = false;
  try {
    await createLibrary(session.pageA);
    const snapshotId = await createPublishedSnapshot(session.pageA, session.hostA, 'Shared');
    await enableMode(session.pageA, session.hostA, 'Cloud Backup', 'Keep in cloud');
    await connectLibrary(session.pageB);
    await enableMode(session.pageB, session.hostB, 'Cloud Backup', 'Keep in cloud');
    await expectDeviceProfiles(seeded.cloudRoot, [DEVICE_A_ID, DEVICE_B_ID]);
    await openGame(session.pageB);
    await downloadSnapshot(session.pageB, snapshotId);
    const archive = await readFile(cloudArchivePath(seeded.cloudRoot, snapshotId));
    const liveSaves = [await readSave(seeded.deviceA), await readSave(seeded.deviceB)];

    await removeLibraryDevice(session.pageA, 'E2E Device B');
    await expectDeviceProfiles(seeded.cloudRoot, [DEVICE_A_ID]);
    expect(existsSync(cloudArchivePath(seeded.cloudRoot, snapshotId))).toBe(true);
    expectDeviceHasNoHead(await readJson(cloudPaths(seeded.cloudRoot).manifest), DEVICE_B_ID);

    await reconnectCloudLibrary(session.pageB);
    await expectDeviceProfiles(seeded.cloudRoot, [DEVICE_A_ID, DEVICE_B_ID]);
    expect(await readFile(cloudArchivePath(seeded.cloudRoot, snapshotId))).toEqual(archive);
    expect(await readFile(localArchivePath(seeded.deviceA.appDataDir, snapshotId))).toEqual(
      archive
    );
    expect(await readFile(localArchivePath(seeded.deviceB.appDataDir, snapshotId))).toEqual(
      archive
    );
    expect([await readSave(seeded.deviceA), await readSave(seeded.deviceB)]).toEqual(liveSaves);
  } catch (error) {
    failed = true;
    throw error;
  } finally {
    await session.close(failed);
  }
});
