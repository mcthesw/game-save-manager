import { test, expect } from '@playwright/test';
import { readFile, rename, unlink, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { DEVICE_A_ID, GAME_NAME } from './support/constants';
import { cloudPaths, readDeviceProfile, readJson } from './support/cloud-assertions';
import { seedEmptyCloudWithLocalGame, writeSave, readSave } from './support/cloud-fixture';
import { applySnapshot, connectLibrary, createLibrary, openApp, openGame } from './support/gui';
import {
  createSnapshotForGame,
  deleteSnapshotViaUi,
  getLocalGame,
  listSnapshotsFor,
  updateGameViaApi,
} from './support/local-gui';
import { createRunRoot, hostPost, newDeviceContext, startRgsmHost } from './support/rgsm-instance';
import { startDualSession } from './support/session';

test('connecting cloud keeps local games usable through refresh, offline edits, and restart', async ({
  browser,
}) => {
  const runRoot = await createRunRoot('local-games');
  const seeded = await seedEmptyCloudWithLocalGame(runRoot, { gameOnB: false });
  const configPath = join(seeded.deviceA.appDataDir, 'GameSaveManager.config.json');
  const config = JSON.parse(await readFile(configPath, 'utf8'));
  config.games[0].cloud_sync_enabled = false;
  config.favorites = [
    { node_id: 'local-favorite', label: GAME_NAME, is_leaf: true, children: null },
  ];
  await writeFile(configPath, JSON.stringify(config));
  const session = await startDualSession(browser, { ...seeded, runRoot, label: 'local-games' });
  let failed = false;
  let restarted: Awaited<ReturnType<typeof startRgsmHost>> | undefined;
  let restartedContext: Awaited<ReturnType<typeof newDeviceContext>> | undefined;
  try {
    await createLibrary(session.pageB);
    await writeSave(seeded.deviceA, 'local original\n');
    const original = await createSnapshotForGame(session.hostA, GAME_NAME, 'Before connection');
    const originalGame = await getLocalGame(session.hostA, GAME_NAME);
    await connectLibrary(session.pageA);
    expect(await getLocalGame(session.hostA, GAME_NAME)).toEqual(originalGame);
    const refresh = await hostPost(session.hostA, '/api/v1/refresh-cloud-archive-library');
    expect(refresh.ok, refresh.raw).toBe(true);
    await openGame(session.pageA);
    await writeSave(seeded.deviceA, 'local later\n');
    const nonHead = await createSnapshotForGame(session.hostA, GAME_NAME, 'After connection');
    await session.pageA.reload();
    await applySnapshot(session.pageA, original);
    expect(await readSave(seeded.deviceA)).toBe('local original\n');
    expect((await readJson(cloudPaths(seeded.cloudRoot).sharedLibrary)).games).toEqual([]);
    expect((await readDeviceProfile(seeded.cloudRoot, DEVICE_A_ID)).games).toEqual({});

    // No page-owned cloud refresh races the isolated offline edit.
    await session.pageA.goto('about:blank');
    await session.pageB.goto('about:blank');
    const settled = await hostPost(session.hostA, '/api/v1/refresh-cloud-archive-library');
    expect(settled.ok, settled.raw).toBe(true);
    const offlineRoot = join(runRoot, 'offline-cloud');
    await rename(seeded.cloudRoot, offlineRoot);
    await writeFile(seeded.cloudRoot, 'Cloud storage is unavailable');
    try {
      const local = await getLocalGame(session.hostA, GAME_NAME);
      await updateGameViaApi(session.hostA, local.storage_key, { ...local, name: 'Renamed local' });
      const permanent = await hostPost(session.hostA, '/api/v1/set-snapshot-created-by', {
        gameName: 'Renamed local',
        snapshotDate: original,
        createdBy: 'Manual',
      });
      expect(permanent.ok, permanent.raw).toBe(true);
      const discardedHead = await createSnapshotForGame(
        session.hostA,
        'Renamed local',
        'Discard this local head'
      );
      await openGame(session.pageA, 'Renamed local');
      await deleteSnapshotViaUi(session.pageA, nonHead);
      await deleteSnapshotViaUi(session.pageA, discardedHead);
      const apiHead = await createSnapshotForGame(session.hostA, 'Renamed local', 'API deletion');
      const deletion = {
        gameId: local.storage_key,
        snapshotId: apiHead,
        currentPosition: { type: 'fallback_to_parent' },
      };
      const unconfirmed = await hostPost(session.hostA, '/api/v1/delete-v2-snapshot', {
        ...deletion,
        confirmed: false,
      });
      expect(unconfirmed.ok).toBe(false);
      const unchanged = await hostPost<{ device_heads: Record<string, string> }>(
        session.hostA,
        '/api/v1/get-game-snapshots-info',
        {
          game: await getLocalGame(session.hostA, 'Renamed local'),
        }
      );
      expect(unchanged.data.device_heads[DEVICE_A_ID]).toBe(apiHead);
      const removed = await hostPost(session.hostA, '/api/v1/delete-v2-snapshot', {
        ...deletion,
        confirmed: true,
      });
      expect(removed.ok, removed.raw).toBe(true);
      expect(
        (await listSnapshotsFor(session.hostA, 'Renamed local')).map((snapshot) => snapshot.date)
      ).toEqual([original]);
      const current = await hostPost<{ device_heads: Record<string, string> }>(
        session.hostA,
        '/api/v1/get-game-snapshots-info',
        {
          game: await getLocalGame(session.hostA, 'Renamed local'),
        }
      );
      expect(current.data.device_heads[DEVICE_A_ID]).toBe(original);
      expect(await readSave(seeded.deviceA)).toBe('local original\n');
    } finally {
      await unlink(seeded.cloudRoot);
      await rename(offlineRoot, seeded.cloudRoot);
    }
    await session.hostA.stop();
    restarted = await startRgsmHost({
      appDataDir: seeded.deviceA.appDataDir,
      deviceId: DEVICE_A_ID,
      logPath: join(runRoot, 'logs', 'local-games-restart.log'),
    });
    restartedContext = await newDeviceContext(browser, restarted);
    await openApp(restartedContext.page);
    await openGame(restartedContext.page, 'Renamed local');
    const restoredGame = await getLocalGame(restarted, 'Renamed local');
    expect(restoredGame.save_paths).toEqual(originalGame.save_paths);
    const afterRename = await hostPost<{ favorites: Array<{ label: string }> }>(
      restarted,
      '/api/v1/get-local-config'
    );
    expect(afterRename.data.favorites[0].label).toBe('Renamed local');
    await createSnapshotForGame(restarted, 'Renamed local', 'After restart');
    expect((await readJson(cloudPaths(seeded.cloudRoot).sharedLibrary)).games).toEqual([]);
  } catch (error) {
    failed = true;
    throw error;
  } finally {
    await restartedContext?.context.close();
    await restarted?.stop();
    await session.close(failed);
  }
});
