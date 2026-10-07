import { test, expect } from '@playwright/test';
import { readFile, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { seedLocalConfig, writeSaveText } from './support/local-fixture';
import { startLocalSession } from './support/local-session';
import { createRunRoot } from './support/rgsm-instance';
import { createSnapshotForGame, getLocalGame } from './support/local-gui';
import { openGame, snapshotRow } from './support/gui';
import { DEVICE_A_ID } from './support/constants';

test('a managed catalog game can be added as a separate installation and account', async ({
  browser,
}, testInfo) => {
  const runRoot = await createRunRoot('add-instance');
  const first = join(runRoot, 'first-install').replaceAll('\\', '/');
  const second = join(runRoot, 'second-install').replaceAll('\\', '/');
  const name = 'Echo Keep';
  const existingName = 'Echo Keep - first account';
  await writeSaveText(`${first}/Profiles/111/profile.sav`, 'first-instance');
  await writeSaveText(`${second}/Profiles/222/profile.sav`, 'second-instance');
  const device = await seedLocalConfig(runRoot, {
    games: [
      { name: existingName, units: [{ type: 'File', path: `${first}/Profiles/111/profile.sav` }] },
    ],
  });
  const configPath = join(device.appDataDir, 'GameSaveManager.config.json');
  const seed = JSON.parse(await readFile(configPath, 'utf8'));
  seed.devices[DEVICE_A_ID].resources = ['111', '222'].map((user_id, id) => ({
    id,
    source: 'manual',
    kind: { type: 'storeAccount', store: 'steam', user_id },
  }));
  seed.devices[DEVICE_A_ID].next_resource_id = 2;
  seed.games[0].ludusavi_meta = {
    installDirs: ['Echo Keep'],
    storeGameIds: [{ store: 'steam', id: '99999001' }],
  };
  seed.games[0].device_bindings = { [DEVICE_A_ID]: { installationPath: first } };
  await writeFile(configPath, JSON.stringify(seed));
  await writeFile(
    join(device.appDataDir, 'ludusavi_manifest.yaml'),
    `
Echo Keep:
  steam:
    id: 99999001
  installDir:
    Echo Keep: {}
  files:
    '<base>/Profiles/<storeUserId>/*.sav':
      tags: [save]
`
  );
  const session = await startLocalSession(browser, { runRoot, device, label: 'add-instance' });
  let failed = false;
  try {
    const { page, host } = session;
    const original = await getLocalGame(host, existingName);
    const originalSnapshot = await createSnapshotForGame(host, existingName, 'first history');
    await page.getByRole('button', { name: 'Add game', exact: true }).first().click();
    await page.getByRole('button', { name: 'Detect local games' }).click();
    const list = page.getByRole('dialog', { name: 'Import Games (Auto-detect Save Locations)' });
    await list.getByRole('checkbox', { name: 'Show only locally installed games' }).uncheck();
    await list.getByRole('checkbox', { name: 'Hide already managed games' }).uncheck();
    await expect(list.getByRole('checkbox', { name, exact: true })).toBeDisabled();
    await list.getByRole('button', { name: 'Add another instance' }).click();
    const customize = page.getByRole('dialog', { name: `Customize Import: ${name}` });
    await expect(customize.getByRole('textbox', { name: 'Game name', exact: true })).toHaveValue(
      `${name} (2)`
    );
    await customize
      .getByRole('textbox', { name: 'Game installation directory', exact: true })
      .fill(second);
    await customize.getByRole('combobox', { name: 'Steam User ID', exact: true }).fill('222');
    await customize.getByRole('button', { name: 'Select all', exact: true }).click();
    await customize.getByRole('button', { name: 'Verify paths', exact: true }).click();
    await expect(
      customize.getByRole('button', { name: 'Verify paths', exact: true })
    ).toBeEnabled();
    await page.setViewportSize({ width: 800, height: 850 });
    await page.screenshot({
      path: testInfo.outputPath('additional-instance-narrow.png'),
      animations: 'disabled',
    });
    await customize.getByRole('button', { name: /^confirm$/i }).click();
    await expect(page.locator(`button[title="${name} (2)"]`)).toBeVisible();
    const added = await getLocalGame(host, `${name} (2)`);
    expect(added.storage_key).not.toBe(original.storage_key);
    expect(await getLocalGame(host, existingName)).toEqual(original);
    const snapshot = await createSnapshotForGame(host, `${name} (2)`, 'second history');
    await writeSaveText(`${second}/Profiles/222/profile.sav`, 'changed-second');
    await page.reload();
    await openGame(page, `${name} (2)`);
    await snapshotRow(page, snapshot).getByRole('button', { name: 'Apply', exact: true }).click();
    await expect
      .poll(() => readFile(`${second}/Profiles/222/profile.sav`, 'utf8'))
      .toBe('second-instance');
    expect(await readFile(`${first}/Profiles/111/profile.sav`, 'utf8')).toBe('first-instance');
    const firstCatalog = JSON.parse(
      await readFile(join(device.archiveRoot, original.storage_key, 'Backups.json'), 'utf8')
    );
    const secondCatalog = JSON.parse(
      await readFile(join(device.archiveRoot, added.storage_key, 'Backups.json'), 'utf8')
    );
    expect(firstCatalog.backups.map((s: { date: string }) => s.date)).toEqual([originalSnapshot]);
    expect(secondCatalog.backups.map((s: { date: string }) => s.date)).toEqual([snapshot]);
  } catch (error) {
    failed = true;
    throw error;
  } finally {
    await session.close(failed);
  }
});
