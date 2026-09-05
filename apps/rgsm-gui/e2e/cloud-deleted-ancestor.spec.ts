import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { GAME_NAME, STORAGE_KEY } from './support/constants';
import { cloudArchivePath, cloudPaths, readJson } from './support/cloud-assertions';
import { seedEmptyCloudWithLocalGame, readSave, writeSave } from './support/cloud-fixture';
import {
  applySnapshot,
  connectLibrary,
  createLibrary,
  createPublishedSnapshot,
  downloadSnapshot,
  evictCloudCopy,
  openGame,
  snapshotRow,
  uploadSnapshot,
} from './support/gui';
import { snapshotMeta, readBackupsJson } from './support/local-assertions';
import { createRunRoot, hostPost } from './support/rgsm-instance';
import { startDualSession } from './support/session';

test('a live descendant remains downloadable and restorable after deleting an ancestor', async ({
  browser,
}) => {
  const runRoot = await createRunRoot('deleted-ancestor');
  const seeded = await seedEmptyCloudWithLocalGame(runRoot);
  const session = await startDualSession(browser, {
    ...seeded,
    runRoot,
    label: 'deleted-ancestor',
  });
  let failed = false;
  try {
    await createLibrary(session.pageA);
    const first = await createPublishedSnapshot(session.pageA, session.hostA, 'Original');
    const ancestor = await createPublishedSnapshot(session.pageA, session.hostA, 'Old progress');
    await writeSave(seeded.deviceA, 'surviving progress\n');
    const child = await createPublishedSnapshot(session.pageA, session.hostA, 'Surviving progress');
    const deletion = await hostPost(session.hostA, '/api/v1/delete-v2-snapshot', {
      gameId: STORAGE_KEY,
      snapshotId: ancestor,
      confirmed: true,
      currentPosition: null,
    });
    expect(deletion.ok, deletion.raw).toBe(true);
    expect(existsSync(cloudArchivePath(seeded.cloudRoot, ancestor))).toBe(false);

    await connectLibrary(session.pageB);
    await openGame(session.pageB);
    await downloadSnapshot(session.pageB, child);
    await applySnapshot(session.pageB, child);
    expect(await readSave(seeded.deviceB)).toBe('surviving progress\n');
    expect((await snapshotMeta(seeded.deviceB.appDataDir, child)).parent).toBe(ancestor);
    expect((await readBackupsJson(seeded.deviceB.appDataDir)).backups.map((s) => s.date)).toEqual([
      first,
      child,
    ]);
    // Uploading an imported copy must not resurrect or rewrite the deleted parent.
    await evictCloudCopy(session.pageB, child);
    await uploadSnapshot(session.pageB, child);
    await writeSave(seeded.deviceB, 'continued on B\n');
    const continued = await createPublishedSnapshot(session.pageB, session.hostB, 'Continued on B');
    expect((await snapshotMeta(seeded.deviceB.appDataDir, continued)).parent).toBe(child);
    await openGame(session.pageA);
    await downloadSnapshot(session.pageA, continued);
    await applySnapshot(session.pageA, continued);
    expect(await readSave(seeded.deviceA)).toBe('continued on B\n');
    await expect(snapshotRow(session.pageA, ancestor)).toHaveCount(0);
    const manifest = await readJson(cloudPaths(seeded.cloudRoot).manifest);
    const game = (
      manifest.games as Record<
        string,
        { snapshots: Record<string, { parent?: string; state: { type: string } }> }
      >
    )[GAME_NAME];
    expect(game.snapshots[ancestor].state.type).toBe('final_tombstone');
    expect(game.snapshots[child].parent).toBe(ancestor);
    expect(game.snapshots[continued].parent).toBe(child);
  } catch (error) {
    failed = true;
    throw error;
  } finally {
    await session.close(failed);
  }
});
