import { test, expect } from '@playwright/test';
import { readFile } from 'node:fs/promises';
import { join } from 'node:path';
import type { Game, Snapshot } from '../src/api/generated/types.gen';
import { createRunRoot, hostPost } from './support/rgsm-instance';
import { seedLocalConfig, writeSaveText } from './support/local-fixture';
import { startLocalSession } from './support/local-session';
import { snapshotRow } from './support/gui';
import { waitForCommand } from './support/command-result';

test('same-title favorites open and restore the selected game independently', async ({
  browser,
}) => {
  const runRoot = await createRunRoot('game-identities');
  const saveA = join(runRoot, 'saves', 'a.sav');
  const saveB = join(runRoot, 'saves', 'b.sav');
  const device = await seedLocalConfig(runRoot, {
    games: [
      { name: 'Same', storageKey: 'first', units: [{ type: 'File', path: saveA }] },
      { name: 'Same', storageKey: 'second', units: [{ type: 'File', path: saveB }] },
    ],
    favorites: [
      { node_id: 'a', label: 'Same', is_leaf: true, game_id: 'first' },
      { node_id: 'b', label: 'Same', is_leaf: true, game_id: 'second' },
    ],
  });
  await writeSaveText(saveA, 'First game');
  await writeSaveText(saveB, 'Second game');
  const session = await startLocalSession(browser, { runRoot, device, label: 'game-identities' });
  try {
    const { page, host } = session;
    await page.locator('.fav-row.leaf').nth(1).click();
    await expect(page).toHaveURL(/gameId=second/);
    await page.reload();
    await expect(page).toHaveURL(/gameId=second/);
    await page.getByPlaceholder('New backup description').fill('Second only');
    const [created] = await Promise.all([
      page.waitForResponse((response) => response.url().endsWith('/api/v1/create-snapshot-at')),
      page.getByRole('button', { name: 'Create new snapshot' }).click(),
    ]);
    expect(created.ok(), await created.text()).toBe(true);
    expect(created.request().postDataJSON().game.storage_key).toBe('second');
    const config = await hostPost<{ games: Game[] }>(host, '/api/v1/get-local-config');
    const second = config.data.games.find((game) => game.storage_key === 'second')!;
    const catalog = await hostPost<{ backups: Snapshot[] }>(
      host,
      '/api/v1/get-game-snapshots-info',
      {
        game: second,
      }
    );
    const snapshot = catalog.data.backups.find((item) => item.describe === 'Second only')!;
    expect(snapshot).toBeTruthy();
    await writeSaveText(saveB, 'Changed second game');
    await waitForCommand(page, '/api/v1/restore-snapshot', snapshot.date, () =>
      snapshotRow(page, snapshot.date).getByRole('button', { name: 'Apply', exact: true }).click()
    );
    expect(await readFile(saveA, 'utf8')).toBe('First game');
    expect(await readFile(saveB, 'utf8')).toBe('Second game');

    const promoted = await hostPost(host, '/api/v1/set-snapshot-created-by', {
      gameId: 'second',
      gameName: 'Same',
      snapshotDate: snapshot.date,
      createdBy: 'Manual',
    });
    expect(promoted.ok, promoted.raw).toBe(true);
    for (const gameId of [undefined, 'missing']) {
      const unresolved = await hostPost(host, '/api/v1/set-snapshot-created-by', {
        gameId,
        gameName: 'Same',
        snapshotDate: snapshot.date,
        createdBy: 'Manual',
      });
      expect(unresolved.ok, unresolved.raw).toBe(false);
    }

    await page.getByRole('button', { name: 'View managed files' }).click();
    const drawer = page.getByRole('dialog');
    // Trimming leaves the existing name unchanged, even though another game shares it.
    await drawer.getByRole('textbox', { name: 'Game name', exact: true }).fill(' Same ');
    const [edit] = await Promise.all([
      page.waitForResponse((response) => response.url().endsWith('/api/v1/update-game'), {
        timeout: 5000,
      }),
      drawer.getByRole('button', { name: 'save', exact: true }).click(),
    ]);
    expect(edit.ok(), await edit.text()).toBe(true);
    expect(edit.request().postDataJSON().storageKey).toBe('second');
    await expect(drawer).not.toBeVisible();
    const renamed = await hostPost(host, '/api/v1/update-game', {
      storageKey: 'second',
      game: {
        name: 'Renamed second',
        save_paths: second.save_paths,
        game_paths: second.game_paths ?? {},
      },
    });
    expect(renamed.ok, renamed.raw).toBe(true);
    // The saved old-title URL still identifies the renamed game.
    await page.reload();
    await expect(page.getByRole('heading', { name: 'Renamed second', exact: true })).toBeVisible();
    await expect(page.locator('.fav-row.leaf').nth(0)).toHaveText('Same');
    await expect(page.locator('.fav-row.leaf').nth(1)).toHaveText('Renamed second');
    const first = config.data.games.find((game) => game.storage_key === 'first')!;
    const removed = await hostPost(host, '/api/v1/delete-game', { game: first });
    expect(removed.ok, removed.raw).toBe(true);
    await page.reload();
    await expect(page.locator('.fav-row.leaf')).toHaveCount(1);
    await expect(page.locator('.fav-row.leaf')).toHaveText('Renamed second');
    await expect(snapshotRow(page, snapshot.date)).toBeVisible();
  } finally {
    await session.close();
  }
});
