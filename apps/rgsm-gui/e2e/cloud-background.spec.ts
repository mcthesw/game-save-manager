import { test, expect } from '@playwright/test';
import { existsSync } from 'node:fs';
import { cloudArchivePath, cloudPaths } from './support/cloud-assertions';
import { enableMode, createSnapshotViaApi, latestSnapshotId } from './support/gui';
import { readFile, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { seedEmptyCloudWithLocalGame } from './support/cloud-fixture';
import { startDualSession } from './support/session';
import { startJoinCodeWebDav } from './support/join-code-webdav';
import { createRunRoot, hostPost } from './support/rgsm-instance';
import { createLibrary, getLocalConfig, listSnapshots } from './support/gui';

test('local backup, description and restore complete while a cloud request is blocked', async ({
  browser,
}) => {
  const runRoot = await createRunRoot('background-cloud');
  const scene = await seedEmptyCloudWithLocalGame(runRoot);
  let blocked = false;
  let entered = false;
  let release = () => {};
  const gate = new Promise<void>((resolve) => {
    release = resolve;
  });
  const dav = await startJoinCodeWebDav(async () => {
    if (blocked) {
      entered = true;
      await gate;
    }
  });
  for (const device of [scene.deviceA, scene.deviceB]) {
    const file = join(device.appDataDir, 'GameSaveManager.config.json');
    const config = JSON.parse(await readFile(file, 'utf8'));
    config.settings.cloud_settings.backend = dav.backend;
    config.settings.cloud_settings.root_path = '/';
    await writeFile(file, JSON.stringify(config));
  }
  const session = await startDualSession(browser, { runRoot, ...scene, label: 'background-cloud' });
  let failed = false;
  let refresh: ReturnType<typeof hostPost> | undefined;
  try {
    await createLibrary(session.pageA);
    const game = (await getLocalConfig(session.hostA)).games[0];
    blocked = true;
    refresh = hostPost(session.hostA, '/api/v1/refresh-cloud-archive-library');
    await expect.poll(() => entered).toBe(true);
    // The deadline distinguishes local completion from waiting for the held request.
    const local = async (path: string, body: unknown) => {
      const result = await Promise.race([
        hostPost(session.hostA, `/api/v1/${path}`, body),
        new Promise<never>((_, reject) => {
          const timeout = setTimeout(
            () => reject(new Error(`Local ${path} waited for cloud`)),
            5000
          );
          timeout.unref();
        }),
      ]);
      expect(result.ok, result.raw).toBe(true);
      return result;
    };
    await local('create-snapshot', { game, describe: 'While cloud is blocked' });
    const snapshots = await listSnapshots(session.hostA);
    const id = snapshots.find((snapshot) => snapshot.describe === 'While cloud is blocked')!.date;
    await local('set-snapshot-description', { game, date: id, describe: 'Offline edit' });
    await writeFile(scene.deviceA.savePath, 'changed live save');
    await local('restore-snapshot', { game, date: id });
    expect(
      (await listSnapshots(session.hostA)).find((snapshot) => snapshot.date === id)?.describe
    ).toBe('Offline edit');
    blocked = false;
    release();
    expect((await refresh).ok).toBe(true);
    await expect
      .poll(() =>
        [...dav.files.values()].some((bytes) => {
          try {
            return JSON.stringify(JSON.parse(bytes.toString())).includes('Offline edit');
          } catch {
            return false;
          }
        })
      )
      .toBe(true);
  } catch (error) {
    failed = true;
    throw error;
  } finally {
    blocked = false;
    release();
    await refresh?.catch(() => {});
    await session.close(failed);
    await dav.close();
  }
});

test('manual refresh finishes pending archive synchronization after an offline failure', async ({
  browser,
}, testInfo) => {
  const runRoot = await createRunRoot('manual-sync-retry');
  const scene = await seedEmptyCloudWithLocalGame(runRoot);
  const session = await startDualSession(browser, {
    ...scene,
    runRoot,
    label: 'manual-sync-retry',
  });
  let failed = false;
  const manifestPath = cloudPaths(scene.cloudRoot).manifest;
  let saved: Buffer | undefined;
  try {
    await createLibrary(session.pageA);
    await enableMode(session.pageA, session.hostA, 'Cloud Backup', 'Keep in cloud');
    await session.pageA.evaluate(async () => {
      const eventPath = '/src/api/events.ts';
      const { events } = await import(eventPath);
      const state = window as Window & { syncFailed?: boolean };
      state.syncFailed = false;
      await events.cloudSyncErrorEvent.listen(() => {
        state.syncFailed = true;
      });
    });
    saved = await readFile(manifestPath);
    await writeFile(manifestPath, 'offline test');
    await createSnapshotViaApi(session.hostA, 'Offline backup');
    const id = await latestSnapshotId(session.hostA, 'Offline backup');
    await expect
      .poll(() =>
        session.pageA.evaluate(() => (window as Window & { syncFailed?: boolean }).syncFailed)
      )
      .toBe(true);
    await session.pageA.setViewportSize({ width: 1440, height: 1000 });
    await session.pageA.locator('.activity-pill').click();
    await expect(session.pageA.locator('.activity-drawer')).toContainText('Cloud sync failed');
    await session.pageA.screenshot({
      path: testInfo.outputPath('local-success-cloud-failure.png'),
      animations: 'disabled',
    });
    await session.pageA.locator('.activity-pill').click();
    expect(existsSync(cloudArchivePath(scene.cloudRoot, id))).toBe(false);
    await writeFile(manifestPath, saved);
    saved = undefined;
    const refreshed = await hostPost(session.hostA, '/api/v1/refresh-cloud-archive-library');
    expect(refreshed.ok, refreshed.raw).toBe(true);
    expect(existsSync(cloudArchivePath(scene.cloudRoot, id))).toBe(true);
  } catch (error) {
    failed = true;
    throw error;
  } finally {
    if (saved) await writeFile(manifestPath, saved);
    await session.close(failed);
  }
});
