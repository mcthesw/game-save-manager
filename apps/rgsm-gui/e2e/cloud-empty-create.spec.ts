import { test, expect } from '@playwright/test';
import { existsSync } from 'node:fs';
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
  createLibrary,
  createPublishedSnapshot,
  downloadSnapshot,
  getGeneration,
  openGame,
} from './support/gui';
import { createRunRoot } from './support/rgsm-instance';
import { startDualSession } from './support/session';

test('empty cloud creates library then first snapshot uploads', async ({ browser }) => {
  const runRoot = await createRunRoot('empty-create');
  const seeded = await seedEmptyCloudWithLocalGame(runRoot, { gameOnB: false });
  const session = await startDualSession(browser, { ...seeded, runRoot, label: 'empty-create' });
  let failed = false;
  try {
    await createLibrary(session.pageA);
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
