import { test, expect } from '@playwright/test';
import { existsSync } from 'node:fs';
import { rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { STORAGE_KEY } from './support/constants';
import {
  cloudArchivePath,
  cloudPaths,
  expectLocalGeneration,
  expectNamespaceDescriptor,
  expectSharedLibraryHasGame,
  localArchivePath,
  readJson,
} from './support/cloud-assertions';
import { seedEmptyCloudWithLocalGame } from './support/cloud-fixture';
import {
  connectLibrary,
  createPublishedSnapshot,
  downloadSnapshot,
  getGeneration,
  openSyncSettings,
  expectLibraryKind,
  openGame,
} from './support/gui';
import { createRunRoot } from './support/rgsm-instance';
import { startDualSession } from './support/session';

test('empty cloud distinguishes a failed create, retries and uploads', async ({
  browser,
}, testInfo) => {
  const runRoot = await createRunRoot('empty-create');
  const seeded = await seedEmptyCloudWithLocalGame(runRoot, { gameOnB: false });
  const session = await startDualSession(browser, { ...seeded, runRoot, label: 'empty-create' });
  let failed = false;
  try {
    await openSyncSettings(session.pageA);
    await expectLibraryKind(session.pageA, 'empty');
    await expect(
      session.pageA.getByText('Could not create the Cloud Library', { exact: true })
    ).toHaveCount(0, { timeout: 5000 });
    await expect(session.pageA.getByRole('button', { name: 'Retry creation' })).toHaveCount(0);
    await session.pageA.screenshot({ path: testInfo.outputPath('acceptance-empty-library.png') });

    // Make the real filesystem backend reject creation, then remove only this test blocker.
    const blocker = join(seeded.cloudRoot, 'v2');
    await writeFile(blocker, 'Creation blocked by a file');
    await session.pageA.getByRole('button', { name: 'Create library', exact: true }).click();
    const main = session.pageA.getByRole('main');
    await expect(
      main.getByText('Could not create the Cloud Library', { exact: true })
    ).toBeVisible();
    await expect(main.getByRole('button', { name: 'Create library', exact: true })).toHaveCount(0);
    await rm(blocker);
    await main.getByRole('button', { name: 'Retry creation' }).click();
    await expectLibraryKind(session.pageA, 'active');
    expect(await getGeneration(session.hostA)).toBe('v2');
    await expectLocalGeneration(seeded.deviceA.appDataDir, 'v2');
    const paths = cloudPaths(seeded.cloudRoot);
    expect(existsSync(paths.namespace)).toBe(true);
    expectNamespaceDescriptor(await readJson(paths.namespace));
    expectSharedLibraryHasGame(await readJson(paths.sharedLibrary));
    expect(existsSync(join(seeded.cloudRoot, 'v2', 'archives', STORAGE_KEY))).toBe(false);

    const snapshotId = await createPublishedSnapshot(session.pageA, session.hostA, 'First upload');
    expect(existsSync(cloudArchivePath(seeded.cloudRoot, snapshotId))).toBe(true);
    expect(existsSync(localArchivePath(seeded.deviceA.appDataDir, snapshotId))).toBe(true);

    await connectLibrary(session.pageB);
    expect(await getGeneration(session.hostB)).toBe('v2');
    await openGame(session.pageB);
    await downloadSnapshot(session.pageB, snapshotId);
    expect(existsSync(localArchivePath(seeded.deviceB.appDataDir, snapshotId))).toBe(true);
  } catch (error) {
    failed = true;
    throw error;
  } finally {
    await session.close(failed);
  }
});
