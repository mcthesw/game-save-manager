import { test, expect } from '@playwright/test';
import { copyFile, mkdir, readFile, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { seedEmptyCloudWithLocalGame } from './support/cloud-fixture';
import { cloudPaths, readJson } from './support/cloud-assertions';
import { GAME_NAME, STORAGE_KEY } from './support/constants';
import { createLibrary, expectLibraryKind } from './support/gui';
import { getLocalGame, updateGameViaApi } from './support/local-gui';
import { createRunRoot, hostPost } from './support/rgsm-instance';
import { startDualSession } from './support/session';

for (const choice of ['cloud', 'local'] as const) {
  test(`a genuine definition conflict requires an explicit ${choice} choice`, async ({
    browser,
  }) => {
    const runRoot = await createRunRoot(`definition-${choice}`);
    const seeded = await seedEmptyCloudWithLocalGame(runRoot);
    const storageKey = choice === 'cloud' ? 'constructor' : '__proto__';
    for (const device of [seeded.deviceA, seeded.deviceB]) {
      const configPath = join(device.appDataDir, 'GameSaveManager.config.json');
      const config = JSON.parse(await readFile(configPath, 'utf8'));
      config.games[0].storage_key = storageKey;
      config.games[0].cloud_sync_enabled = false;
      if (device === seeded.deviceA) config.games[0].name = 'Local title';
      await writeFile(configPath, JSON.stringify(config));
      await mkdir(join(device.archiveRoot, storageKey));
      await copyFile(
        join(device.archiveRoot, STORAGE_KEY, 'Backups.json'),
        join(device.archiveRoot, storageKey, 'Backups.json')
      );
    }
    const session = await startDualSession(browser, { ...seeded, runRoot, label: 'definition' });
    let failed = false;
    try {
      await createLibrary(session.pageB);
      const before = await getLocalGame(session.hostA, 'Local title');
      const omitted = await hostPost<{ kind: string }>(
        session.hostA,
        '/api/v1/join-cloud-library',
        {
          decisions: [],
          confirmedReplacements: false,
        }
      );
      expect(omitted.ok, omitted.raw).toBe(true);
      expect(omitted.data.kind).toBe('review_changed');
      expect(await getLocalGame(session.hostA, 'Local title')).toEqual(before);
      await session.pageA.goto('/SyncSettings');
      await expectLibraryKind(session.pageA, 'join');
      await session.pageA.getByRole('button', { name: 'Join library' }).first().click();
      const dialog = session.pageA.getByRole('dialog', { name: 'Confirm Cloud Library join' });
      await expect(dialog).toBeVisible();
      const submit = dialog.getByRole('button', { name: 'Join library', exact: true });
      await expect(submit).toBeDisabled();
      const option = dialog.getByRole('button', {
        name: choice === 'cloud' ? 'Keep the cloud version' : 'Replace the shared game definition',
        exact: true,
      });
      await expect(option).toHaveAttribute('aria-pressed', 'false');
      await option.click();
      await expect(submit).toBeEnabled();
      await submit.click();
      if (choice === 'local') {
        await session.pageA.getByRole('button', { name: 'Replace and join', exact: true }).click();
      }
      await expect(dialog).toBeHidden();
      const expectedName = choice === 'cloud' ? GAME_NAME : 'Local title';
      const accepted = await getLocalGame(session.hostA, expectedName);
      expect(accepted.storage_key).toBe(storageKey);
      expect(accepted.save_paths).toEqual(before.save_paths);
      const remote = await readJson(cloudPaths(seeded.cloudRoot).sharedLibrary);
      expect(remote.games).toEqual([
        expect.objectContaining({ storage_key: storageKey, name: expectedName }),
      ]);
    } catch (error) {
      failed = true;
      throw error;
    } finally {
      await session.close(failed);
    }
  });
}

test('a definition changed after an equal review reloads the pending choice', async ({
  browser,
}) => {
  const runRoot = await createRunRoot('definition-changed');
  const seeded = await seedEmptyCloudWithLocalGame(runRoot);
  const session = await startDualSession(browser, {
    ...seeded,
    runRoot,
    label: 'definition-changed',
  });
  let failed = false;
  try {
    await createLibrary(session.pageB);
    await session.pageA.goto('/SyncSettings');
    await expectLibraryKind(session.pageA, 'join');
    await session.pageA.getByRole('button', { name: 'Join library' }).first().click();
    const original = session.pageA.getByRole('dialog', { name: 'Join Cloud Library', exact: true });
    await expect(original.getByRole('button', { name: 'Join library', exact: true })).toBeEnabled();
    const game = await getLocalGame(session.hostB, GAME_NAME);
    await updateGameViaApi(session.hostB, STORAGE_KEY, { ...game, name: 'Changed cloud title' });
    const remoteBefore = await readFile(cloudPaths(seeded.cloudRoot).sharedLibrary);
    await original.getByRole('button', { name: 'Join library', exact: true }).click();
    const changed = session.pageA.getByRole('dialog', { name: 'Confirm Cloud Library join' });
    await expect(changed).toBeVisible();
    await expect(changed.getByText('Changed cloud title', { exact: true })).toBeVisible();
    await expect(changed.getByRole('button', { name: 'Join library', exact: true })).toBeDisabled();
    expect(await readFile(cloudPaths(seeded.cloudRoot).sharedLibrary)).toEqual(remoteBefore);
    expect((await getLocalGame(session.hostA, GAME_NAME)).name).toBe(GAME_NAME);
  } catch (error) {
    failed = true;
    throw error;
  } finally {
    await session.close(failed);
  }
});
