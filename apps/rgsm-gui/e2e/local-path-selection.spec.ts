import { test, expect } from '@playwright/test';
import { readFile, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { spawnSync } from 'node:child_process';
import { seedLocalConfig, writeSaveText } from './support/local-fixture';
import { startLocalSession } from './support/local-session';
import { createRunRoot, hostPost } from './support/rgsm-instance';
import { openGame } from './support/gui';
import { applySnapshotViaApi, getLocalGame, listSnapshotsFor } from './support/local-gui';
import { DEVICE_A_ID, GAME_NAME } from './support/constants';

test('root selection stays local to the form and drives preview and backup', async ({
  browser,
}, testInfo) => {
  const runRoot = await createRunRoot('path-selection');
  const rootA = join(runRoot, 'library-a').replaceAll('\\', '/');
  const rootB = join(runRoot, 'library-b').replaceAll('\\', '/');
  const device = await seedLocalConfig(runRoot, {
    games: [{ name: GAME_NAME, units: [{ type: 'File', path: `${rootB}/slot[1].sav` }] }],
  });
  await writeSaveText(`${rootB}/slot[1].sav`, 'selected library progress');
  const launcherSource = join(runRoot, 'launcher.rs');
  await writeFile(
    launcherSource,
    'fn main() { std::fs::write("launched.txt", "selected library").unwrap(); }'
  );
  const compiled = spawnSync('rustc', [launcherSource, '-o', `${rootB}/game.exe`], {
    windowsHide: true,
  });
  expect(compiled.status, compiled.stderr.toString()).toBe(0);
  const configPath = join(device.appDataDir, 'GameSaveManager.config.json');
  const seed = JSON.parse(await readFile(configPath, 'utf8'));
  seed.devices[DEVICE_A_ID].resources = [rootA, rootB].map((path, id) => ({
    id,
    source: 'manual',
    kind: { type: 'gameRoot', store: 'other', path },
  }));
  seed.devices[DEVICE_A_ID].next_resource_id = 2;
  await writeFile(configPath, JSON.stringify(seed));
  const session = await startLocalSession(browser, { runRoot, device, label: 'path-selection' });
  let failed = false;
  try {
    const { page, host } = session;
    await page.getByRole('button', { name: 'Add game', exact: true }).first().click();
    let dialog = page.getByRole('dialog');
    await dialog.locator('.pvi-editor').first().fill('<root>/game.exe');
    await expect(dialog).toContainText('Select a location');
    await dialog.getByRole('combobox', { name: 'Game root directory', exact: true }).click();
    await page.getByText(`other · ${rootB}`, { exact: true }).click();
    await dialog.getByRole('heading', { level: 2 }).click();
    await expect(dialog).toContainText(`${rootB}/game.exe`);
    await page.screenshot({ path: testInfo.outputPath('path-selection-add.png') });
    // Closing a draft must not write a game or a device-wide selection.
    await page.keyboard.press('Escape');
    await expect(dialog).toBeHidden();
    await openGame(page);
    await page.getByRole('button', { name: 'View managed files' }).click();
    dialog = page.getByRole('dialog');
    const saveEditor = dialog.locator('.pvi-editor').nth(1);
    await saveEditor.fill('<root>/slot[[]1[]].sav');
    await expect(dialog.locator('.pvi-status--error')).toHaveCount(1);
    await dialog.getByRole('combobox', { name: 'Game root directory', exact: true }).click();
    await page.getByText(`other · ${rootB}`, { exact: true }).click();
    await dialog.getByRole('heading', { level: 2 }).click();
    await expect(dialog.locator('.pvi-status--ok')).toHaveCount(1);
    await dialog.locator('.pvi-editor').first().fill('<root>/game.exe');
    await dialog.getByRole('button', { name: 'save', exact: true }).click();
    await expect
      .poll(async () => (await getLocalGame(host, GAME_NAME)).device_bindings)
      .toMatchObject({ [DEVICE_A_ID]: { rootIds: [1] } });
    await page.keyboard.press('Escape');
    await page.getByRole('button', { name: 'Start game', exact: true }).click();
    await expect
      .poll(async () => readFile(`${rootB}/launched.txt`, 'utf8').catch(() => ''))
      .toBe('selected library');
    const saved = await getLocalGame(host, GAME_NAME);
    const checked = await hostPost<{ status: string; resolvedPath: string }[]>(
      host,
      '/api/v1/check-paths',
      {
        paths: ['<root>/slot[1].sav'],
        game: saved,
        literal: true,
      }
    );
    expect(checked.ok, checked.raw).toBe(true);
    expect(checked.data[0]).toMatchObject({ status: 'ok', resolvedPath: `${rootB}/slot[1].sav` });
    await page.getByRole('button', { name: 'Create new snapshot' }).click();
    await expect.poll(async () => (await listSnapshotsFor(host, GAME_NAME)).length).toBe(1);
    const [snapshot] = await listSnapshotsFor(host, GAME_NAME);
    await writeSaveText(`${rootB}/slot[1].sav`, 'changed progress');
    await applySnapshotViaApi(host, GAME_NAME, snapshot.date);
    expect(await readFile(`${rootB}/slot[1].sav`, 'utf8')).toBe('selected library progress');
    await page.reload();
    await openGame(page);
    await page.getByRole('button', { name: 'View managed files' }).click();
    await expect(page.getByRole('dialog').locator('.pvi-editor').nth(1)).toHaveText(
      '<root>/slot[[]1[]].sav'
    );
    await expect(page.getByRole('dialog').locator('.pvi-status--ok')).toHaveCount(2);
    await page.screenshot({ path: testInfo.outputPath('path-selection-managed.png') });
  } catch (error) {
    failed = true;
    throw error;
  } finally {
    await session.close(failed);
  }
});
