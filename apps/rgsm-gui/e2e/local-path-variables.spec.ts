import { test, expect } from '@playwright/test';
import { readFile, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { seedLocalConfig, writeSaveText } from './support/local-fixture';
import { startLocalSession } from './support/local-session';
import { createRunRoot, hostPost } from './support/rgsm-instance';
import { openGame } from './support/gui';
import { applySnapshotViaApi, createSnapshotForGame, getLocalGame } from './support/local-gui';
import { DEVICE_A_ID, GAME_NAME } from './support/constants';

test('game variables inherit, preview drafts, cancel, save defaults and restore every glob match', async ({
  browser,
}, testInfo) => {
  const runRoot = await createRunRoot('path-variables');
  const root = join(runRoot, 'saves').replaceAll('\\', '/');
  for (const account of ['shared', 'local', 'changed']) {
    for (const slot of ['first', 'second'])
      await writeSaveText(`${root}/${account}/${slot}.sav`, `${account}-${slot}`);
  }
  const device = await seedLocalConfig(runRoot, {
    games: [GAME_NAME, 'Other game'].map((name) => ({
      name,
      units: [{ type: 'File', path: `${root}/shared/first.sav` }],
    })),
  });
  const configPath = join(device.appDataDir, 'GameSaveManager.config.json');
  const seed = JSON.parse(await readFile(configPath, 'utf8'));
  seed.devices[DEVICE_A_ID].path_variables = { account: 'shared' };
  for (const game of seed.games)
    game.save_paths[0].source = {
      type: 'devicePaths',
      unit_type: 'File',
      paths: { [DEVICE_A_ID]: `${root}/<var:account>/*.sav` },
    };
  await writeFile(configPath, JSON.stringify(seed));
  const session = await startLocalSession(browser, { runRoot, device, label: 'variables' });
  let failed = false;
  try {
    const { page, host } = session;
    const config = async () => (await hostPost<any>(host, '/api/v1/get-local-config')).data;
    await openGame(page);
    await page.getByRole('button', { name: 'View managed files' }).click();
    const dialog = page.getByRole('dialog');
    const account = dialog.locator('[data-variable="account"]');
    await expect(account.getByText('Device variable', { exact: true })).toBeVisible();
    await account.getByRole('textbox', { name: 'Value of account' }).fill('local');
    await expect(dialog.locator('.pvi-status--ok')).toHaveCount(1);
    await dialog.getByRole('button', { name: 'cancel', exact: true }).click();
    await expect(account.getByRole('textbox')).toHaveValue('shared');
    expect((await config()).devices[DEVICE_A_ID].path_variables.account).toBe('shared');
    await account.getByRole('textbox', { name: 'Value of account' }).fill('local');
    await dialog.getByRole('button', { name: 'save', exact: true }).click();
    await expect
      .poll(async () => (await getLocalGame(host, GAME_NAME)).device_bindings)
      .toMatchObject({ [DEVICE_A_ID]: { pathVariables: { account: 'local' } } });
    expect((await config()).devices[DEVICE_A_ID].path_variables.account).toBe('shared');
    const snapshot = await createSnapshotForGame(host, GAME_NAME, 'two slots');
    await writeSaveText(`${root}/local/first.sav`, 'changed');
    await writeSaveText(`${root}/local/second.sav`, 'changed');
    await applySnapshotViaApi(host, GAME_NAME, snapshot);
    expect(await readFile(`${root}/local/first.sav`, 'utf8')).toBe('local-first');
    expect(await readFile(`${root}/local/second.sav`, 'utf8')).toBe('local-second');
    await page.reload();
    await openGame(page);
    await page.getByRole('button', { name: 'View managed files' }).click();
    await expect(account.getByRole('textbox', { name: 'Value of account' })).toHaveValue('local');
    await account.getByRole('button', { name: 'Actions for account' }).click();
    await page.getByRole('menuitem', { name: 'Use device variable', exact: true }).click();
    const before = await account.boundingBox();
    await account.getByRole('button', { name: 'Actions for account' }).click();
    await page.getByRole('menuitem', { name: 'Edit device variable…', exact: true }).click();
    const defaults = page.getByRole('dialog', { name: 'Edit device variable…', exact: true });
    await defaults.getByRole('textbox').fill('discarded');
    await defaults.getByRole('button', { name: 'cancel', exact: true }).click();
    await expect(account.getByRole('textbox')).toHaveValue('shared');
    await account.getByRole('button', { name: 'Actions for account' }).click();
    await page.getByRole('menuitem', { name: 'Edit device variable…', exact: true }).click();
    await defaults.getByRole('textbox').fill('changed');
    await expect(defaults.getByText('Other game', { exact: true })).toBeVisible();
    await page.screenshot({
      path: testInfo.outputPath('edit-device-default.png'),
      animations: 'disabled',
    });
    await defaults.getByRole('button', { name: 'Use this value' }).click();
    await expect(defaults).not.toBeVisible();
    await expect(account.getByRole('textbox')).toHaveValue('changed');
    expect(await account.boundingBox()).toEqual(before);
    // Draft previews must resolve the new value before persistence.
    await expect
      .poll(async () => (await config()).devices[DEVICE_A_ID].path_variables.account)
      .toBe('shared');
    await page.screenshot({
      path: testInfo.outputPath('variables-default.png'),
      animations: 'disabled',
    });
    await page.setViewportSize({ width: 640, height: 900 });
    await page.screenshot({
      path: testInfo.outputPath('variables-narrow.png'),
      animations: 'disabled',
    });
    await dialog.getByRole('button', { name: 'save', exact: true }).click();
    await expect
      .poll(async () => (await config()).devices[DEVICE_A_ID].path_variables.account)
      .toBe('changed');
    expect((await getLocalGame(host, GAME_NAME)).device_bindings).not.toMatchObject({
      [DEVICE_A_ID]: { pathVariables: { account: 'local' } },
    });
  } catch (error) {
    failed = true;
    throw error;
  } finally {
    await session.close(failed);
  }
});

test('Add game inserts a newly created variable into the intended path', async ({
  browser,
}, testInfo) => {
  const runRoot = await createRunRoot('add-variable');
  const root = join(runRoot, 'saves').replaceAll('\\', '/');
  await writeSaveText(`${root}/player/first.sav`, 'first');
  await writeSaveText(`${root}/player/second.sav`, 'second');
  const device = await seedLocalConfig(runRoot, { games: [] });
  const session = await startLocalSession(browser, { runRoot, device, label: 'add-variable' });
  let failed = false;
  try {
    const { page, host } = session;
    await page.getByRole('button', { name: 'Add game', exact: true }).first().click();
    const dialog = page.getByRole('dialog', { name: 'Add Game' });
    await dialog.getByPlaceholder('Please enter the game name (required)').fill('Typed game');
    await dialog.getByRole('button', { name: 'Add save file', exact: true }).click();
    await expect(dialog.locator('.pvi-editor')).toHaveCount(2);
    await dialog.locator('.pvi-editor').nth(1).fill(`${root}/`);
    await dialog.getByRole('button', { name: 'Insert variable', exact: true }).nth(1).click();
    await page.getByRole('button', { name: 'Create variable and insert' }).click();
    const variableDialog = page.getByRole('dialog', { name: 'Add variable', exact: true });
    await variableDialog
      .getByRole('textbox', { name: 'Variable name', exact: true })
      .fill('account');
    await variableDialog
      .getByRole('textbox', { name: 'Variable value', exact: true })
      .fill('player');
    await variableDialog.getByRole('button', { name: 'Add variable', exact: true }).click();
    await dialog.locator('.pvi-editor').nth(1).press('End');
    await dialog.locator('.pvi-editor').nth(1).pressSequentially('/*.sav');
    await expect(dialog.locator('.pvi-status--ok')).toHaveCount(1);
    await page.screenshot({
      path: testInfo.outputPath('add-game-variables.png'),
      animations: 'disabled',
    });
    await dialog.getByRole('button', { name: 'save', exact: true }).click();
    await expect(dialog).not.toBeVisible();
    await expect
      .poll(async () => (await getLocalGame(host, 'Typed game')).device_bindings)
      .toMatchObject({ [DEVICE_A_ID]: { pathVariables: { account: 'player' } } });
    await createSnapshotForGame(host, 'Typed game', 'typed glob');
  } catch (error) {
    failed = true;
    throw error;
  } finally {
    await session.close(failed);
  }
});

test('missing variables offer candidates, cancel safely and resume backup and restore', async ({
  browser,
}) => {
  const runRoot = await createRunRoot('missing-variables');
  const root = join(runRoot, 'saves').replaceAll('\\', '/');
  await writeSaveText(`${root}/player/first.sav`, 'first');
  await writeSaveText(`${root}/other/first.sav`, 'other');
  const device = await seedLocalConfig(runRoot, {
    games: [{ name: GAME_NAME, units: [{ type: 'File', path: `${root}/<var:account>/*.sav` }] }],
  });
  const session = await startLocalSession(browser, { runRoot, device, label: 'missing-variables' });
  let failed = false;
  try {
    const { page, host } = session;
    await openGame(page);
    await expect(page.getByRole('button', { name: 'Set up variables', exact: true })).toBeVisible();
    await page.getByRole('button', { name: 'Create new snapshot', exact: true }).click();
    const drawer = page.getByRole('dialog');
    await expect(drawer.getByRole('button', { name: 'Save and continue' })).toBeVisible();
    await drawer.getByRole('button', { name: 'Close', exact: true }).click();
    await expect(drawer).not.toBeVisible();
    await page.getByRole('button', { name: 'Create new snapshot', exact: true }).click();
    await drawer.getByRole('button', { name: 'Find local location', exact: true }).click();
    const discovery = page.getByRole('dialog', {
      name: 'Find a local value for account',
      exact: true,
    });
    await expect(discovery.getByRole('radio')).toHaveCount(2);
    await expect(discovery.getByRole('button', { name: 'Use for this game' })).toBeDisabled();
    await discovery.getByRole('radio').last().check();
    await discovery.getByRole('button', { name: 'Use for this game' }).click();
    await expect(drawer.getByRole('textbox', { name: 'Value of account' })).toHaveValue('player');
    expect(
      ((await getLocalGame(host, GAME_NAME)) as any).device_bindings?.[DEVICE_A_ID]?.pathVariables
        ?.account
    ).toBeUndefined();
    await drawer.getByRole('button', { name: 'Save and continue' }).click();
    await expect(drawer).not.toBeVisible();
    await expect
      .poll(async () => {
        const game = await getLocalGame(host, GAME_NAME);
        return (await hostPost<any>(host, '/api/v1/get-game-snapshots-info', { game })).data
          ?.backups?.length;
      })
      .toBe(1);
    await expect(
      page.getByRole('button', { name: 'Set up variables', exact: true })
    ).not.toBeVisible();
    const config = (await hostPost<any>(host, '/api/v1/get-local-config')).data;
    delete config.games[0].device_bindings[DEVICE_A_ID].pathVariables.account;
    config.settings.confirm_before_apply_latest = false;
    const saved = await hostPost(host, '/api/v1/set-config', { config });
    expect(saved.ok, saved.raw).toBe(true);
    await page.reload();
    await openGame(page);
    await page.getByRole('button', { name: 'Apply latest', exact: true }).click();
    await expect(drawer.getByRole('button', { name: 'Save and continue' })).toBeVisible();
    await drawer.getByRole('textbox', { name: 'Value of account' }).fill('new-player');
    await drawer.getByRole('button', { name: 'Save and continue' }).click();
    await expect
      .poll(async () => readFile(`${root}/new-player/first.sav`, 'utf8').catch(() => ''))
      .toBe('first');
    await page.route('**/api/v1/missing-game-variables', (route) =>
      route.fulfill({
        status: 503,
        contentType: 'application/json',
        body: JSON.stringify({ message: 'Unavailable' }),
      })
    );
    await page.getByRole('button', { name: 'Create new snapshot', exact: true }).click();
    await expect(
      page
        .getByText(
          'Could not check path variables on this device. No operation was performed. Please try again.',
          { exact: true }
        )
        .first()
    ).toBeVisible();
    await expect(drawer).not.toBeVisible();
    const after = await hostPost<any>(host, '/api/v1/get-game-snapshots-info', {
      game: await getLocalGame(host, GAME_NAME),
    });
    expect(after.data.backups).toHaveLength(1);
  } catch (error) {
    failed = true;
    throw error;
  } finally {
    await session.close(failed);
  }
});
