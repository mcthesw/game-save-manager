import { test, expect } from '@playwright/test';
import { readFile, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { cloudArchivePath, cloudPaths, readJson } from './support/cloud-assertions';
import { seedEmptyCloudWithLocalGame, readSave } from './support/cloud-fixture';
import { STORAGE_KEY } from './support/constants';
import {
  changeGameMode,
  connectLibrary,
  createLibrary,
  createPublishedSnapshot,
  downloadArchiveViaApi,
  getLocalConfig,
  listSnapshots,
  openGame,
  snapshotRow,
  uploadArchiveViaApi,
} from './support/gui';
import { createRunRoot, hostPost, startRgsmHost, type RgsmHost } from './support/rgsm-instance';
import { startDualSession } from './support/session';

type Manifest = {
  games: Record<
    string,
    {
      device_heads: Record<string, string>;
      snapshots: Record<string, { description: string }>;
    }
  >;
};

test('editing a shared description updates existing copies without transferring saves', async ({
  browser,
}) => {
  const runRoot = await createRunRoot('description');
  const seeded = await seedEmptyCloudWithLocalGame(runRoot);
  const session = await startDualSession(browser, { ...seeded, runRoot, label: 'description' });
  let failed = false;
  try {
    await createLibrary(session.pageA);
    const id = await createPublishedSnapshot(session.pageA, session.hostA, 'Original description');
    await connectLibrary(session.pageB);
    await downloadArchiveViaApi(session.hostB, id);
    await changeGameMode(session.pageA, 'Manual');
    const archive = await readFile(cloudArchivePath(seeded.cloudRoot, id));
    const saves = await Promise.all([readSave(seeded.deviceA), readSave(seeded.deviceB)]);
    const manifest = async () =>
      (await readJson(cloudPaths(seeded.cloudRoot).manifest)) as unknown as Manifest;
    const before = await manifest();
    await openGame(session.pageA);
    await snapshotRow(session.pageA, id)
      .getByRole('button', { name: 'Modify', exact: true })
      .click();
    const dialog = session.pageA.getByRole('dialog', { name: 'Enter new description' });
    await expect(dialog.getByRole('textbox')).toHaveValue('Original description');
    await dialog.getByRole('textbox').fill('Revised description');
    await dialog.getByRole('button', { name: 'Confirm', exact: true }).click();
    await expect
      .poll(async () => {
        const data = await manifest();
        return data.games[STORAGE_KEY]!.snapshots[id]!.description;
      })
      .toBe('Revised description');
    // A stale local copy must not overwrite an explicit edit during upload.
    await uploadArchiveViaApi(session.hostB, id);
    const refreshed = await hostPost(session.hostB, '/api/v1/refresh-cloud-archive-library');
    expect(refreshed.ok, refreshed.raw).toBe(true);
    expect((await listSnapshots(session.hostB)).find((s) => s.date === id)?.describe).toBe(
      'Revised description'
    );
    await openGame(session.pageB);
    await expect(snapshotRow(session.pageB, id)).toContainText('Revised description');
    // Empty text is a real edit, not missing metadata.
    const game = (await getLocalConfig(session.hostB)).games[0];
    const cleared = await hostPost(session.hostB, '/api/v1/set-snapshot-description', {
      game,
      date: id,
      describe: '',
    });
    expect(cleared.ok, cleared.raw).toBe(true);
    await expect
      .poll(async () => (await manifest()).games[STORAGE_KEY]!.snapshots[id]!.description)
      .toBe('');
    await hostPost(session.hostA, '/api/v1/refresh-cloud-archive-library');
    expect((await listSnapshots(session.hostA)).find((s) => s.date === id)?.describe).toBe('');
    const after = await manifest();
    expect(after.games[STORAGE_KEY]!.device_heads).toEqual(before.games[STORAGE_KEY]!.device_heads);
    expect(after.games[STORAGE_KEY]!.snapshots[id]).toEqual({
      ...before.games[STORAGE_KEY]!.snapshots[id],
      description: '',
    });
    expect(await readFile(cloudArchivePath(seeded.cloudRoot, id))).toEqual(archive);
    expect(await Promise.all([readSave(seeded.deviceA), readSave(seeded.deviceB)])).toEqual(saves);
  } catch (error) {
    failed = true;
    throw error;
  } finally {
    await session.close(failed);
  }
});

test('failed description sync survives restart and retries on refresh', async ({ browser }) => {
  const runRoot = await createRunRoot('description-retry');
  const seeded = await seedEmptyCloudWithLocalGame(runRoot);
  const session = await startDualSession(browser, {
    ...seeded,
    runRoot,
    label: 'description-retry',
  });
  let restarted: RgsmHost | undefined;
  let failed = false;
  const manifestPath = cloudPaths(seeded.cloudRoot).manifest;
  let manifestBytes: Buffer | undefined;
  try {
    await createLibrary(session.pageA);
    const id = await createPublishedSnapshot(session.pageA, session.hostA, 'Before offline edit');
    await openGame(session.pageA);
    manifestBytes = await readFile(manifestPath);
    // Only the isolated test backend is made unreadable, never the user's cloud.
    await writeFile(manifestPath, 'unreadable test manifest');
    await snapshotRow(session.pageA, id)
      .getByRole('button', { name: 'Modify', exact: true })
      .click();
    const dialog = session.pageA.getByRole('dialog', { name: 'Enter new description' });
    await dialog.getByRole('textbox').fill('Saved while offline');
    const saved = session.pageA.waitForResponse(
      (response) =>
        response.url().endsWith('/api/v1/set-snapshot-description') &&
        response.request().method() === 'POST'
    );
    await dialog.getByRole('button', { name: 'Confirm', exact: true }).click();
    expect((await (await saved).json()).cloud_sync_pending).toBe(true);
    await expect(dialog).toBeHidden();
    expect((await listSnapshots(session.hostA)).find((s) => s.date === id)?.describe).toBe(
      'Saved while offline'
    );
    await session.contextA.close();
    await session.hostA.stop();
    await writeFile(manifestPath, manifestBytes);
    manifestBytes = undefined;
    restarted = await startRgsmHost({
      appDataDir: seeded.deviceA.appDataDir,
      deviceId: seeded.deviceA.id,
      logPath: join(runRoot, 'logs', 'restarted.log'),
    });
    const result = await hostPost(restarted, '/api/v1/refresh-cloud-archive-library');
    expect(result.ok, result.raw).toBe(true);
    const remote = (await readJson(manifestPath)) as unknown as Manifest;
    expect(remote.games[STORAGE_KEY]!.snapshots[id]!.description).toBe('Saved while offline');
    const config = await getLocalConfig(restarted);
    const local = await hostPost<{ pending_descriptions?: Record<string, unknown> }>(
      restarted,
      '/api/v1/get-game-snapshots-info',
      { game: config.games[0] }
    );
    expect(local.ok, local.raw).toBe(true);
    expect(local.data.pending_descriptions ?? {}).toEqual({});
  } catch (error) {
    failed = true;
    throw error;
  } finally {
    if (manifestBytes) await writeFile(manifestPath, manifestBytes);
    await restarted?.stop();
    await session.close(failed);
  }
});
