import { test, expect } from '@playwright/test';
import { readFile, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { seedLocalConfig, writeSaveText } from './support/local-fixture';
import { startLocalSession } from './support/local-session';
import { createRunRoot } from './support/rgsm-instance';
import { openGame, snapshotRow } from './support/gui';
import { createSnapshotForGame, getLocalGame } from './support/local-gui';
import { DEVICE_A_ID, GAME_NAME } from './support/constants';

test('one library selection restores every wildcard file to the current game instance', async ({
  browser,
}, testInfo) => {
  const runRoot = await createRunRoot('single-game-instance');
  const libraries = ['original-library', 'another-library-with-a-long-name-for-layout-checks'].map(
    (name) => join(runRoot, name).replaceAll('\\', '/')
  );
  for (const [index, library] of libraries.entries()) {
    await writeSaveText(`${library}/Saves/one.sav`, `library-${index}-one`);
    await writeSaveText(`${library}/Saves/two.sav`, `library-${index}-two`);
  }
  const device = await seedLocalConfig(runRoot, {
    games: [{ name: GAME_NAME, units: [{ type: 'File', path: `${libraries[0]}/Saves/one.sav` }] }],
  });
  const configPath = join(device.appDataDir, 'GameSaveManager.config.json');
  const seed = JSON.parse(await readFile(configPath, 'utf8'));
  seed.devices[DEVICE_A_ID].resources = libraries.map((path, id) => ({
    id,
    source: 'manual',
    kind: { type: 'gameRoot', store: 'steam', path },
  }));
  seed.devices[DEVICE_A_ID].next_resource_id = 2;
  seed.games[0].save_paths[0].source = {
    type: 'manifestPattern',
    pattern: '<root>/Saves/*.sav',
    unit_type: 'File',
  };
  seed.games[0].device_bindings = { [DEVICE_A_ID]: { rootIds: [0] } };
  await writeFile(configPath, JSON.stringify(seed));
  const session = await startLocalSession(browser, { runRoot, device, label: 'single-instance' });
  let failed = false;
  try {
    const { page, host } = session;
    const snapshot = await createSnapshotForGame(host, GAME_NAME, 'both slots');
    await writeSaveText(`${libraries[0]}/Saves/one.sav`, 'keep-original-instance');
    await openGame(page);
    await page.getByRole('button', { name: 'View managed files' }).click();
    const drawer = page.getByRole('dialog');
    const library = drawer.getByRole('combobox', { name: 'Game root directory' });
    await library.click();
    await page.getByRole('option', { name: `Steam · ${libraries[1]}`, exact: true }).click();
    await page.screenshot({
      path: testInfo.outputPath('library-wide.png'),
      animations: 'disabled',
    });
    await page.setViewportSize({ width: 800, height: 800 });
    const box = await library.boundingBox();
    expect(box).not.toBeNull();
    expect(box!.x + box!.width).toBeLessThanOrEqual(800);
    await page.screenshot({
      path: testInfo.outputPath('library-narrow.png'),
      animations: 'disabled',
    });
    await drawer.getByRole('button', { name: 'save', exact: true }).click();
    await expect
      .poll(async () => (await getLocalGame(host, GAME_NAME)).device_bindings)
      .toMatchObject({
        [DEVICE_A_ID]: { rootIds: ['resource:1'] },
      });
    await page.setViewportSize({ width: 1280, height: 800 });
    await snapshotRow(page, snapshot).getByRole('button', { name: 'Apply', exact: true }).click();
    await expect
      .poll(() => readFile(`${libraries[1]}/Saves/one.sav`, 'utf8'))
      .toBe('library-0-one');
    expect(await readFile(`${libraries[1]}/Saves/two.sav`, 'utf8')).toBe('library-0-two');
    expect(await readFile(`${libraries[0]}/Saves/one.sav`, 'utf8')).toBe('keep-original-instance');
  } catch (error) {
    failed = true;
    throw error;
  } finally {
    await session.close(failed);
  }
});

test('a manual installation supplies base and restores to its edited directory', async ({
  browser,
}, testInfo) => {
  const runRoot = await createRunRoot('manual-installation');
  const original = join(runRoot, 'a standalone installation with a long folder name').replaceAll(
    '\\',
    '/'
  );
  const target = join(runRoot, 'moved-installation').replaceAll('\\', '/');
  await writeSaveText(`${original}/Saves/one.sav`, 'original-slot');
  await writeSaveText(`${target}/Saves/one.sav`, 'target-slot');
  const device = await seedLocalConfig(runRoot, {
    games: [{ name: GAME_NAME, units: [{ type: 'File', path: `${original}/Saves/one.sav` }] }],
  });
  const configPath = join(device.appDataDir, 'GameSaveManager.config.json');
  const seed = JSON.parse(await readFile(configPath, 'utf8'));
  seed.games[0].save_paths[0].source = {
    type: 'manifestPattern',
    pattern: '<base>/Saves/*.sav',
    unit_type: 'File',
  };
  await writeFile(configPath, JSON.stringify(seed));
  const session = await startLocalSession(browser, {
    runRoot,
    device,
    label: 'manual-installation',
  });
  let failed = false;
  try {
    const { page, host } = session;
    await openGame(page);
    await page.getByRole('button', { name: 'View managed files' }).click();
    const drawer = page.getByRole('dialog');
    await drawer
      .getByRole('textbox', { name: 'Game installation directory', exact: true })
      .fill(original);
    await page.setViewportSize({ width: 800, height: 800 });
    await page.screenshot({
      path: testInfo.outputPath('manual-installation-narrow.png'),
      animations: 'disabled',
    });
    await drawer.getByRole('button', { name: 'save', exact: true }).click();
    await expect
      .poll(async () => (await getLocalGame(host, GAME_NAME)).device_bindings)
      .toMatchObject({ [DEVICE_A_ID]: { installationPath: original } });
    const snapshot = await createSnapshotForGame(host, GAME_NAME, 'manual base');
    await page.reload();
    await openGame(page);
    await page.getByRole('button', { name: 'View managed files' }).click();
    await drawer
      .getByRole('textbox', { name: 'Game installation directory', exact: true })
      .fill(target);
    await drawer.getByRole('button', { name: 'save', exact: true }).click();
    await expect
      .poll(async () => (await getLocalGame(host, GAME_NAME)).device_bindings)
      .toMatchObject({ [DEVICE_A_ID]: { installationPath: target } });
    await snapshotRow(page, snapshot).getByRole('button', { name: 'Apply', exact: true }).click();
    await expect.poll(() => readFile(`${target}/Saves/one.sav`, 'utf8')).toBe('original-slot');
    expect(await readFile(`${original}/Saves/one.sav`, 'utf8')).toBe('original-slot');
  } catch (error) {
    failed = true;
    throw error;
  } finally {
    await session.close(failed);
  }
});
