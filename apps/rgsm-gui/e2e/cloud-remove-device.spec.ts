import { test, expect } from '@playwright/test';
import { existsSync } from 'node:fs';
import { DEVICE_A_ID, DEVICE_B_ID } from './support/constants';
import {
  cloudArchivePath,
  cloudPaths,
  expectDeviceHasNoHead,
  expectDeviceProfiles,
  readJson,
} from './support/cloud-assertions';
import { seedEmptyCloudWithLocalGame } from './support/cloud-fixture';
import {
  confirmJoinKeepCloud,
  createLibrary,
  createPublishedSnapshot,
  enableMode,
  removeLibraryDevice,
} from './support/gui';
import { createRunRoot } from './support/rgsm-instance';
import { startDualSession } from './support/session';

test('remove other device from library', async ({ browser }) => {
  const runRoot = await createRunRoot('remove-device');
  const seeded = await seedEmptyCloudWithLocalGame(runRoot);
  const session = await startDualSession(browser, { ...seeded, runRoot, label: 'remove-device' });
  let failed = false;
  try {
    await createLibrary(session.pageA);
    const snapshotId = await createPublishedSnapshot(session.pageA, session.hostA, 'Shared');
    await enableMode(session.pageA, session.hostA, 'Cloud Backup', 'Keep in cloud');
    await confirmJoinKeepCloud(session.pageB);
    await enableMode(session.pageB, session.hostB, 'Cloud Backup', 'Keep in cloud');
    await expectDeviceProfiles(seeded.cloudRoot, [DEVICE_A_ID, DEVICE_B_ID]);

    await removeLibraryDevice(session.pageA, 'E2E Device B');
    await expectDeviceProfiles(seeded.cloudRoot, [DEVICE_A_ID]);
    expect(existsSync(cloudArchivePath(seeded.cloudRoot, snapshotId))).toBe(true);
    expectDeviceHasNoHead(await readJson(cloudPaths(seeded.cloudRoot).manifest), DEVICE_B_ID);
  } catch (error) {
    failed = true;
    throw error;
  } finally {
    await session.close(failed);
  }
});
