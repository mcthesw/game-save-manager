import { test, expect } from '@playwright/test';
import { copyFile, mkdir, readFile, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { seedEmptyCloudWithLocalGame, readSave } from './support/cloud-fixture';
import { createLibrary, connectLibrary, uploadSnapshot, downloadSnapshot } from './support/gui';
import { createSnapshotForGame } from './support/local-gui';
import { createRunRoot, hostPost } from './support/rgsm-instance';
import { startDualSession } from './support/session';
import { cloudPaths, readJson } from './support/cloud-assertions';

test('a retained local game can enable cloud sync and send a snapshot to the other device', async ({
  browser,
}, testInfo) => {
  const runRoot = await createRunRoot('local-game-cloud');
  const scene = await seedEmptyCloudWithLocalGame(runRoot);
  const file = join(scene.deviceB.appDataDir, 'GameSaveManager.config.json');
  const config = JSON.parse(await readFile(file, 'utf8'));
  config.games[0].name = 'Private game';
  config.games[0].storage_key = 'private-game';
  config.games[0].save_paths[0].paths = {
    [scene.deviceB.id]: scene.deviceB.savePath.replaceAll('\\', '/'),
  };
  await writeFile(file, JSON.stringify(config));
  await mkdir(join(scene.deviceB.archiveRoot, 'private-game'));
  await copyFile(
    join(scene.deviceB.archiveRoot, 'Echo Keep/Backups.json'),
    join(scene.deviceB.archiveRoot, 'private-game/Backups.json')
  );
  const session = await startDualSession(browser, { ...scene, runRoot, label: 'local-game-cloud' });
  let failed = false;
  try {
    await createLibrary(session.pageA);
    await connectLibrary(session.pageB);
    const originalSave = await readSave(scene.deviceB);
    const before = await hostPost(session.hostB, '/api/v1/get-local-config');
    const row = session.pageB.locator('[data-game-id="private-game"]');
    await expect(row.getByText('Local only', { exact: true })).toBeVisible();
    await expect(row.getByRole('switch')).not.toBeChecked();
    const remoteBefore = await readJson(cloudPaths(scene.cloudRoot).sharedLibrary);
    await row.getByRole('switch').click();
    const dialog = session.pageB.getByRole('dialog', { name: 'Enable cloud sync', exact: true });
    await expect(
      dialog.getByText('You can then find Private game on your other devices', { exact: true })
    ).toBeVisible();
    await session.pageB.screenshot({
      path: testInfo.outputPath('acceptance-enable-cloud-sync.png'),
    });
    await dialog.getByRole('button', { name: 'Cancel', exact: true }).click();
    expect(await readJson(cloudPaths(scene.cloudRoot).sharedLibrary)).toEqual(remoteBefore);
    expect((await hostPost(session.hostB, '/api/v1/get-local-config')).data).toEqual(before.data);
    await row.getByRole('switch').click();
    await dialog.getByRole('button', { name: 'Enable cloud sync', exact: true }).click();
    await expect(dialog).toBeHidden();
    await expect(row.getByRole('switch')).toBeChecked();
    await expect(row.getByRole('button', { name: 'Sync mode', exact: true })).toContainText(
      'Manual'
    );
    await session.pageB.screenshot({ path: testInfo.outputPath('local-game-enabled.png') });
    const snapshot = await createSnapshotForGame(session.hostB, 'Private game', 'From device B');
    await session.pageB.goto('/Management/Private%20game?gameId=private-game');
    await uploadSnapshot(session.pageB, snapshot);
    await session.pageA.goto('/SyncSettings');
    await expect(session.pageA.locator('[data-game-id="private-game"]')).toBeVisible();
    await session.pageA
      .locator('[data-game-id="private-game"]')
      .getByRole('button', { name: 'Private game', exact: true })
      .click();
    await downloadSnapshot(session.pageA, snapshot);
    const aConfig = await hostPost<{ games: Array<{ name: string; storage_key: string }> }>(
      session.hostA,
      '/api/v1/get-local-config'
    );
    expect(aConfig.data.games.some((game) => game.storage_key === 'private-game')).toBe(true);
    expect(await readSave(scene.deviceB)).toBe(originalSave);
    expect(await readSave(scene.deviceA)).toBe(originalSave);
    await session.pageB.reload();
    await session.pageB.goto('/SyncSettings');
    await expect(row.getByRole('switch')).toBeChecked();
  } catch (error) {
    failed = true;
    throw error;
  } finally {
    await session.close(failed);
  }
});
