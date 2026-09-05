import { test, expect } from '@playwright/test';
import { existsSync } from 'node:fs';
import { rm } from 'node:fs/promises';
import {
  cloudArchivePath,
  cloudPaths,
  expectDeviceProfiles,
  expectNamespaceDescriptor,
  expectSharedLibraryHasGame,
  localArchivePath,
  liveSnapshotState,
  readJson,
} from './support/cloud-assertions';
import { seedEmptyCloudWithLocalGame } from './support/cloud-fixture';
import {
  connectLibrary,
  createLibrary,
  createPublishedSnapshot,
  downloadSnapshot,
  expectLibraryKind,
  openGame,
  openSyncSettings,
  rebuildCloudLibrary,
  reconnectCloudLibrary,
} from './support/gui';
import { DEVICE_A_ID, DEVICE_B_ID } from './support/constants';
import { createRunRoot } from './support/rgsm-instance';
import { startDualSession } from './support/session';

test('one device rebuilds a deleted library and the other reconnects without deleting it', async ({
  browser,
}) => {
  const runRoot = await createRunRoot('reset');
  const seeded = await seedEmptyCloudWithLocalGame(runRoot);
  const session = await startDualSession(browser, { ...seeded, runRoot, label: 'reset' });
  let failed = false;
  try {
    await createLibrary(session.pageA);
    const snapshotId = await createPublishedSnapshot(session.pageA, session.hostA, 'Before reset');
    await session.pageB.reload();
    await openSyncSettings(session.pageB);
    await expectLibraryKind(session.pageB, 'active');
    await connectLibrary(session.pageB);
    await openGame(session.pageB);
    await downloadSnapshot(session.pageB, snapshotId);
    expect(existsSync(localArchivePath(seeded.deviceB.appDataDir, snapshotId))).toBe(true);
    expectSharedLibraryHasGame(await readJson(cloudPaths(seeded.cloudRoot).sharedLibrary));

    await rm(cloudPaths(seeded.cloudRoot).sharedLibrary);
    await openSyncSettings(session.pageA);
    await expect(
      session.pageA.getByRole('button', { name: 'Rebuild from this device' })
    ).toBeVisible();
    await rebuildCloudLibrary(session.pageA);
    const paths = cloudPaths(seeded.cloudRoot);
    await expect.poll(() => existsSync(paths.namespace), { timeout: 15_000 }).toBe(true);
    expectNamespaceDescriptor(await readJson(paths.namespace));
    expectSharedLibraryHasGame(await readJson(paths.sharedLibrary));
    liveSnapshotState(await readJson(paths.manifest), snapshotId);
    expect(existsSync(cloudArchivePath(seeded.cloudRoot, snapshotId))).toBe(true);
    await expectDeviceProfiles(seeded.cloudRoot, [DEVICE_A_ID]);

    await session.pageB.reload();
    await openSyncSettings(session.pageB);
    await expect(
      session.pageB.getByRole('button', { name: 'Reconnect this device' })
    ).toBeVisible();
    await expectDeviceProfiles(seeded.cloudRoot, [DEVICE_A_ID]);
    await reconnectCloudLibrary(session.pageB);

    await expectDeviceProfiles(seeded.cloudRoot, [DEVICE_A_ID, DEVICE_B_ID]);
    expectSharedLibraryHasGame(await readJson(paths.sharedLibrary));
    liveSnapshotState(await readJson(paths.manifest), snapshotId);
    expect(existsSync(cloudArchivePath(seeded.cloudRoot, snapshotId))).toBe(true);
    expect(existsSync(localArchivePath(seeded.deviceB.appDataDir, snapshotId))).toBe(true);
  } catch (error) {
    failed = true;
    throw error;
  } finally {
    await session.close(failed);
  }
});
