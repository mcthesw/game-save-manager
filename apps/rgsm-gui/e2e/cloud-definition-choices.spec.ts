import { test, expect } from '@playwright/test';
import { copyFile, mkdir, readFile, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { seedEmptyCloudWithLocalGame } from './support/cloud-fixture';
import { cloudPaths, readJson } from './support/cloud-assertions';
import { GAME_NAME, STORAGE_KEY } from './support/constants';
import { createLibrary, expectLibraryKind } from './support/gui';
import { createSnapshotForGame, getLocalGame, updateGameViaApi } from './support/local-gui';
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
      config.games.push(
        {
          ...structuredClone(config.games[0]),
          storage_key: 'still-pending',
          name: device === seeded.deviceA ? 'Another local' : 'Another cloud',
        },
        { ...structuredClone(config.games[0]), storage_key: 'ready-game', name: 'Ready game' }
      );
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
      await expectLibraryKind(session.pageA, 'active');
      await expect(session.pageA.getByRole('button', { name: 'Join library' })).toHaveCount(0);
      await expect(
        session.pageA.getByRole('button', { name: 'Choose game definition' })
      ).toHaveCount(2);
      const blocked = await hostPost(session.hostA, '/api/v1/upload-cloud-archive', {
        gameId: storageKey,
        snapshotId: 'anything',
      });
      expect(blocked.ok).toBe(false);
      const ready = await createSnapshotForGame(
        session.hostA,
        'Ready game',
        'While another game needs a choice'
      );
      const upload = await hostPost(session.hostA, '/api/v1/upload-cloud-archive', {
        gameId: 'ready-game',
        snapshotId: ready,
      });
      expect(upload.ok, upload.raw).toBe(true);
      const download = await hostPost(session.hostB, '/api/v1/download-cloud-archive', {
        gameId: 'ready-game',
        snapshotId: ready,
      });
      expect(download.ok, download.raw).toBe(true);
      const choose = session.pageA
        .locator(`[data-game-id="${storageKey}"]`)
        .getByRole('button', { name: 'Choose game definition' });
      await expect(choose).toBeVisible();
      await choose.click();
      const dialog = session.pageA.getByRole('dialog', { name: 'Choose game definition' });
      await expect(dialog).toBeVisible();
      const submit = dialog.getByRole('button', { name: 'Use selected definition', exact: true });
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
        await session.pageA
          .getByRole('button', { name: 'Use selected definition', exact: true })
          .last()
          .click();
      }
      await expect(dialog).toBeHidden();
      const expectedName = choice === 'cloud' ? GAME_NAME : 'Local title';
      const accepted = await getLocalGame(session.hostA, expectedName);
      expect(accepted.storage_key).toBe(storageKey);
      expect(accepted.save_paths).toEqual(before.save_paths);
      const remote = await readJson(cloudPaths(seeded.cloudRoot).sharedLibrary);
      expect(remote.games).toEqual(
        expect.arrayContaining([
          expect.objectContaining({ storage_key: storageKey, name: expectedName }),
          expect.objectContaining({ storage_key: 'still-pending', name: 'Another cloud' }),
        ])
      );
      expect((await getLocalGame(session.hostA, 'Another local')).name).toBe('Another local');
      await expect(
        session.pageA.getByRole('button', { name: 'Choose game definition' })
      ).toHaveCount(1);
      await session.pageA.reload();
      await expectLibraryKind(session.pageA, 'active');
      await expect(
        session.pageA.getByRole('button', { name: 'Choose game definition' })
      ).toHaveCount(1);
    } catch (error) {
      failed = true;
      throw error;
    } finally {
      await session.close(failed);
    }
  });
}

test('a definition changed during selection reloads the pending choice', async ({ browser }) => {
  const runRoot = await createRunRoot('definition-changed');
  const seeded = await seedEmptyCloudWithLocalGame(runRoot);
  const configPath = join(seeded.deviceA.appDataDir, 'GameSaveManager.config.json');
  const config = JSON.parse(await readFile(configPath, 'utf8'));
  config.games[0].name = 'Local title';
  config.games[0].cloud_sync_enabled = false;
  await writeFile(configPath, JSON.stringify(config));
  const session = await startDualSession(browser, {
    ...seeded,
    runRoot,
    label: 'definition-changed',
  });
  let failed = false;
  try {
    await createLibrary(session.pageB);
    await session.pageA.goto('/SyncSettings');
    await expectLibraryKind(session.pageA, 'active');
    const choose = session.pageA.getByRole('button', { name: 'Choose game definition' });
    await expect(choose).toBeVisible();
    await choose.click();
    const original = session.pageA.getByRole('dialog', {
      name: 'Choose game definition',
      exact: true,
    });
    await original.getByRole('button', { name: 'Keep the cloud version', exact: true }).click();
    await expect(
      original.getByRole('button', { name: 'Use selected definition', exact: true })
    ).toBeEnabled();
    const game = await getLocalGame(session.hostB, GAME_NAME);
    await updateGameViaApi(session.hostB, STORAGE_KEY, { ...game, name: 'Changed cloud title' });
    const remoteBefore = await readFile(cloudPaths(seeded.cloudRoot).sharedLibrary);
    await original.getByRole('button', { name: 'Use selected definition', exact: true }).click();
    const changed = session.pageA.getByRole('dialog', { name: 'Choose game definition' });
    await expect(changed).toBeVisible();
    await expect(changed.getByText('Changed cloud title', { exact: true })).toBeVisible();
    await expect(
      changed.getByRole('button', { name: 'Use selected definition', exact: true })
    ).toBeDisabled();
    expect(await readFile(cloudPaths(seeded.cloudRoot).sharedLibrary)).toEqual(remoteBefore);
    expect((await getLocalGame(session.hostA, 'Local title')).name).toBe('Local title');
    // A remote deletion must invalidate a selection in the already-open dialog.
    // The overview may refresh and remove its cloud row before the player submits.
    await changed.getByRole('button', { name: 'Keep the cloud version', exact: true }).click();
    const deletion = await hostPost(session.hostB, '/api/v1/permanently-delete-cloud-game', {
      gameId: STORAGE_KEY,
      confirmed: true,
    });
    expect(deletion.ok, deletion.raw).toBe(true);
    await changed.getByRole('button', { name: 'Use selected definition', exact: true }).click();
    await expect(
      changed.getByText(
        'The cloud definition is no longer available · Your local game is unchanged'
      )
    ).toBeVisible();
    await expect(
      changed.getByRole('button', { name: 'Use selected definition', exact: true })
    ).toBeDisabled();
    await expect(changed.getByRole('button', { name: 'Add local game', exact: true })).toHaveCount(
      0
    );
    expect((await getLocalGame(session.hostA, 'Local title')).name).toBe('Local title');
  } catch (error) {
    failed = true;
    throw error;
  } finally {
    await session.close(failed);
  }
});
