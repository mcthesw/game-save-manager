import { test, expect } from '@playwright/test';
import { readFile, writeFile } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import { DEVICE_A_ID, GAME_NAME, STORAGE_KEY } from './support/constants';
import { cloudPaths, localArchivePath, localOwnerPaths } from './support/cloud-assertions';
import { seedEmptyCloudWithLocalGame } from './support/cloud-fixture';
import { createLibrary, createPublishedSnapshot } from './support/gui';
import { createRunRoot, hostPost } from './support/rgsm-instance';
import { startDualSession } from './support/session';

test('catalog queries leave local state unchanged until explicit metadata refresh', async ({
  browser,
}) => {
  const runRoot = await createRunRoot('catalog-read');
  const seeded = await seedEmptyCloudWithLocalGame(runRoot);
  const session = await startDualSession(browser, { ...seeded, runRoot, label: 'catalog-read' });
  let failed = false;
  try {
    await createLibrary(session.pageA);
    const snapshotId = await createPublishedSnapshot(session.pageA, session.hostA, 'Known archive');
    // Stop page-owned refreshes while testing the read endpoint in isolation.
    await session.pageA.goto('about:blank');
    await session.pageB.goto('about:blank');
    const settled = await hostPost(session.hostA, '/api/v1/refresh-cloud-archive-library');
    expect(settled.ok, settled.raw).toBe(true);
    const paths = cloudPaths(seeded.cloudRoot);
    const owner = localOwnerPaths(seeded.deviceA.appDataDir, DEVICE_A_ID);
    const archive = localArchivePath(seeded.deviceA.appDataDir, snapshotId);
    const originalArchive = await readFile(archive);
    const originalOwners = await Promise.all(
      [owner.sharedLibrary, owner.profile].map((path) => readFile(path))
    );

    const remoteLibrary = JSON.parse(await readFile(paths.sharedLibrary, 'utf8'));
    remoteLibrary.games[0].name = 'Remote name';
    await writeFile(paths.sharedLibrary, JSON.stringify(remoteLibrary));
    const manifest = JSON.parse(await readFile(paths.manifest, 'utf8'));
    const game = manifest.games[STORAGE_KEY];
    game.snapshots[snapshotId].state = { type: 'final_tombstone', kind: 'user' };
    game.device_heads = {};
    const remoteBytes = Buffer.from(JSON.stringify(manifest));
    await writeFile(paths.manifest, remoteBytes);

    for (const path of ['/api/v1/get-cloud-archive-library', '/api/v1/preview-materialize-all']) {
      const result = await hostPost(session.hostA, path);
      expect(result.ok, result.raw).toBe(true);
      expect(await readFile(archive)).toEqual(originalArchive);
      expect(await readFile(paths.manifest)).toEqual(remoteBytes);
      expect(
        await Promise.all([owner.sharedLibrary, owner.profile].map((file) => readFile(file)))
      ).toEqual(originalOwners);
    }
    const before = await hostPost<{ games: Array<{ name: string }> }>(
      session.hostA,
      '/api/v1/get-local-config'
    );
    expect(before.data.games[0].name).toBe(GAME_NAME);

    const refresh = await hostPost(session.hostA, '/api/v1/refresh-cloud-archive-library');
    expect(refresh.ok, refresh.raw).toBe(true);
    expect(existsSync(archive)).toBe(false);
    const after = await hostPost<{ games: Array<{ name: string }> }>(
      session.hostA,
      '/api/v1/get-local-config'
    );
    expect(after.data.games[0].name).toBe('Remote name');
    const accepted = JSON.parse(await readFile(paths.manifest, 'utf8'));
    expect(accepted.games[STORAGE_KEY].local_archives[DEVICE_A_ID] ?? []).not.toContain(snapshotId);
  } catch (error) {
    failed = true;
    throw error;
  } finally {
    await session.close(failed);
  }
});
