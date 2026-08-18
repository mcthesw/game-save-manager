import { test, expect } from '@playwright/test';
import { existsSync } from 'node:fs';
import { rm } from 'node:fs/promises';
import {
  cloudPaths,
  expectNamespaceDescriptor,
  expectSharedLibraryHasGame,
  readJson,
} from './support/cloud-assertions';
import { seedEmptyCloudWithLocalGame } from './support/cloud-fixture';
import {
  createLibrary,
  createPublishedSnapshot,
  openSyncSettings,
  resetBrokenLibrary,
} from './support/gui';
import { createRunRoot } from './support/rgsm-instance';
import { startDualSession } from './support/session';

test('broken library reset and recreate', async ({ browser }) => {
  const runRoot = await createRunRoot('reset');
  const seeded = await seedEmptyCloudWithLocalGame(runRoot);
  const session = await startDualSession(browser, { ...seeded, runRoot, label: 'reset' });
  let failed = false;
  try {
    await createLibrary(session.pageA);
    await createPublishedSnapshot(session.pageA, session.hostA, 'Before reset');
    expectSharedLibraryHasGame(await readJson(cloudPaths(seeded.cloudRoot).sharedLibrary));

    await rm(cloudPaths(seeded.cloudRoot).sharedLibrary);
    await openSyncSettings(session.pageA);
    await expect(session.pageA.getByRole('button', { name: 'Reset and recreate' })).toBeVisible();
    await resetBrokenLibrary(session.pageA);
    const paths = cloudPaths(seeded.cloudRoot);
    expect(existsSync(paths.namespace)).toBe(true);
    expectNamespaceDescriptor(await readJson(paths.namespace));
    expectSharedLibraryHasGame(await readJson(paths.sharedLibrary));
  } catch (error) {
    failed = true;
    throw error;
  } finally {
    await session.close(failed);
  }
});
