import { test, expect } from '@playwright/test';
import { access, readFile, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { seedLocalConfig, writeSaveText } from './support/local-fixture';
import { startLocalSession } from './support/local-session';
import { createRunRoot, hostPost } from './support/rgsm-instance';
import { openGame } from './support/gui';
import { applySnapshotViaApi, createSnapshotForGame, getLocalGame } from './support/local-gui';
import { DEVICE_A_ID, GAME_NAME } from './support/constants';

for (const [kind, typed] of [
  ['File', true],
  ['Folder', true],
  ['File', false],
  ['Folder', false],
] as const) {
  test(`device ${typed ? 'typed' : 'dynamic'} ${kind.toLowerCase()} override replaces the pattern and can be cleared`, async ({
    browser,
  }, testInfo) => {
    const runRoot = await createRunRoot('path-override');
    const original = join(runRoot, 'original').replaceAll('\\', '/');
    const local = join(runRoot, 'renamed').replaceAll('\\', '/');
    const sourceFile = kind === 'File' ? original : `${original}/slot.sav`;
    const localFile = kind === 'File' ? local : `${local}/slot.sav`;
    const device = await seedLocalConfig(runRoot, {
      games: [
        {
          name: GAME_NAME,
          units: [
            { type: kind, path: original },
            { type: 'Folder', path: `${runRoot}/settings` },
          ],
        },
      ],
    });
    await writeSaveText(sourceFile, 'original progress');
    await writeSaveText(`${runRoot}/settings/options.ini`, 'volume=80');
    const configPath = join(device.appDataDir, 'GameSaveManager.config.json');
    const seed = JSON.parse(await readFile(configPath, 'utf8'));
    const unitId = seed.games[0].save_paths[0].id;
    seed.games[0].save_paths[0].source = {
      type: 'manifestPattern',
      pattern: original,
      expected_type: typed ? kind : null,
    };
    seed.games[0].device_bindings = {
      'other-device': {
        pathOverrides: { [unitId]: { expression: '/other-device/save' } },
      },
    };
    await writeFile(configPath, JSON.stringify(seed));
    const session = await startLocalSession(browser, { runRoot, device, label: 'override' });
    let failed = false;
    try {
      const { page, host } = session;
      const beforeOverride = await createSnapshotForGame(host, GAME_NAME, 'before override');
      await openGame(page);
      await page.getByRole('button', { name: 'View managed files' }).click();
      const dialog = page.getByRole('dialog');
      await dialog
        .getByRole('switch', { name: 'Override path on this device', exact: true })
        .check();
      await dialog.locator('.pvi-editor').nth(1).fill(local);
      await dialog.getByRole('button', { name: 'cancel', exact: true }).click();
      await expect(dialog.locator('.pvi-editor').nth(1)).toHaveText(original);
      await dialog
        .getByRole('switch', { name: 'Override path on this device', exact: true })
        .check();
      await dialog.locator('.pvi-editor').nth(1).fill(local);
      await dialog.getByRole('button', { name: 'save', exact: true }).click();
      await expect
        .poll(async () => (await getLocalGame(host, GAME_NAME)).device_bindings)
        .toMatchObject({
          [DEVICE_A_ID]: { pathOverrides: { [unitId]: { expression: local } } },
          'other-device': {
            pathOverrides: { [unitId]: { expression: '/other-device/save' } },
          },
        });
      const saved = await getLocalGame(host, GAME_NAME);
      expect(saved.save_paths[0].source).toMatchObject({
        type: 'manifestPattern',
        pattern: original,
      });
      const missing = await hostPost(host, '/api/v1/create-snapshot', {
        game: saved,
        describe: 'must fail',
      });
      expect(missing.ok, 'missing override must not silently back up the original path').toBe(
        false
      );
      await applySnapshotViaApi(host, GAME_NAME, beforeOverride);
      expect(await readFile(localFile, 'utf8')).toBe('original progress');
      await writeSaveText(sourceFile, 'original must stay untouched');
      await writeSaveText(localFile, 'local progress');
      const overridden = await createSnapshotForGame(host, GAME_NAME, 'local override');
      await writeSaveText(localFile, 'changed');
      await applySnapshotViaApi(host, GAME_NAME, overridden);
      expect(await readFile(localFile, 'utf8')).toBe('local progress');
      expect(await readFile(sourceFile, 'utf8')).toBe('original must stay untouched');
      await page.reload();
      await openGame(page);
      await page.getByRole('button', { name: 'View managed files' }).click();
      await expect(dialog.locator('.pvi-editor').nth(1)).toHaveText(local);
      await expect(dialog.locator('.pvi-status--ok')).toHaveCount(2);
      await page.screenshot({
        path: testInfo.outputPath('device-path-override.png'),
        animations: 'disabled',
      });
      // Keep long paths and multiple entries usable in a narrower drawer.
      await page.setViewportSize({ width: 640, height: 900 });
      await expect(
        dialog.getByRole('switch', { name: 'Override path on this device' })
      ).toBeChecked();
      await expect(dialog.getByRole('combobox', { name: 'Path type' })).toHaveCount(0);
      await page.screenshot({
        path: testInfo.outputPath('device-path-override-narrow.png'),
        animations: 'disabled',
      });
      await page.setViewportSize({ width: 1280, height: 720 });
      await dialog
        .getByRole('switch', { name: 'Override path on this device', exact: true })
        .uncheck();
      await expect(dialog.locator('.pvi-editor').nth(1)).toHaveText(original);
      await dialog.getByRole('button', { name: 'save', exact: true }).click();
      await expect
        .poll(async () => JSON.stringify((await getLocalGame(host, GAME_NAME)).device_bindings))
        .not.toContain(local);
      await writeSaveText(localFile, 'inactive override');
      await applySnapshotViaApi(host, GAME_NAME, overridden);
      expect(await readFile(sourceFile, 'utf8')).toBe('local progress');
      expect(await readFile(localFile, 'utf8')).toBe('inactive override');
      await createSnapshotForGame(host, GAME_NAME, 'original again');
    } catch (error) {
      failed = true;
      throw error;
    } finally {
      await session.close(failed);
    }
  });
}

test('multiple captured files cannot be restored onto one override file', async ({ browser }) => {
  const runRoot = await createRunRoot('override-multiple');
  const root = join(runRoot, 'saves').replaceAll('\\', '/');
  const local = join(runRoot, 'override.sav').replaceAll('\\', '/');
  const device = await seedLocalConfig(runRoot, {
    games: [{ name: GAME_NAME, units: [{ type: 'File', path: `${root}/first.sav` }] }],
  });
  await writeSaveText(`${root}/first.sav`, 'first');
  await writeSaveText(`${root}/second.sav`, 'second');
  const configPath = join(device.appDataDir, 'GameSaveManager.config.json');
  const seed = JSON.parse(await readFile(configPath, 'utf8'));
  seed.devices[DEVICE_A_ID].resources = [
    { id: 0, source: 'manual', kind: { type: 'gameRoot', store: 'other', path: root } },
  ];
  seed.devices[DEVICE_A_ID].next_resource_id = 1;
  seed.games[0].device_bindings = { [DEVICE_A_ID]: { rootIds: [0] } };
  seed.games[0].save_paths[0].source = {
    type: 'manifestPattern',
    pattern: '<root>/*.sav',
    expected_type: 'File',
  };
  await writeFile(configPath, JSON.stringify(seed));
  const session = await startLocalSession(browser, { runRoot, device, label: 'override-multiple' });
  let failed = false;
  try {
    const { host } = session;
    const date = await createSnapshotForGame(host, GAME_NAME, 'two files');
    const game = await getLocalGame(host, GAME_NAME);
    game.device_bindings = {
      [DEVICE_A_ID]: {
        pathOverrides: { [game.save_paths[0].id]: { expression: local } },
      },
    };
    const saved = await hostPost(host, '/api/v1/update-game', {
      storageKey: game.storage_key,
      game,
    });
    expect(saved.ok, saved.raw).toBe(true);
    const restored = await hostPost(host, '/api/v1/restore-snapshot', { game, date });
    expect(restored.ok, restored.raw).toBe(false);
    expect(restored.raw).toContain('Snapshot location does not match the override');
    expect(
      await access(local).then(
        () => true,
        () => false
      )
    ).toBe(false);
    expect(await readFile(`${root}/first.sav`, 'utf8')).toBe('first');
    expect(await readFile(`${root}/second.sav`, 'utf8')).toBe('second');
  } catch (error) {
    failed = true;
    throw error;
  } finally {
    await session.close(failed);
  }
});

test('override switches preserve the game root and surrounding layout', async ({ browser }) => {
  const runRoot = await createRunRoot('override-layout');
  const root = join(runRoot, 'library').replaceAll('\\', '/');
  const device = await seedLocalConfig(runRoot, {
    games: [
      {
        name: GAME_NAME,
        units: [
          { type: 'File', path: `${root}/save.dat` },
          { type: 'File', path: `${root}/other.dat` },
        ],
      },
    ],
  });
  await writeSaveText(`${root}/save.dat`, 'progress');
  await writeSaveText(`${root}/other.dat`, 'other progress');
  const configPath = join(device.appDataDir, 'GameSaveManager.config.json');
  const seed = JSON.parse(await readFile(configPath, 'utf8'));
  seed.devices[DEVICE_A_ID].resources = [root, `${runRoot}/other`].map((path, id) => ({
    id,
    source: 'manual',
    kind: { type: 'gameRoot', store: 'other', path },
  }));
  seed.devices[DEVICE_A_ID].next_resource_id = 2;
  seed.games[0].save_paths[0].source = {
    type: 'manifestPattern',
    pattern: '<root>/save.dat',
    expected_type: 'File',
  };
  seed.games[0].device_bindings = { [DEVICE_A_ID]: { rootIds: [0] } };
  await writeFile(configPath, JSON.stringify(seed));
  const session = await startLocalSession(browser, { runRoot, device, label: 'override-layout' });
  let failed = false;
  try {
    const { page } = session;
    await openGame(page);
    await page.getByRole('button', { name: 'View managed files' }).click();
    const dialog = page.getByRole('dialog');
    const rootSelect = dialog.getByRole('button', { name: 'Game root directory', exact: true });
    const toggle = dialog.getByRole('switch', { name: 'Override path on this device' });
    const saveEditor = dialog.locator('.pvi-editor').nth(1);
    const checkedPaths: string[] = [];
    page.on('request', (request) => {
      if (request.url().endsWith('/check-paths'))
        checkedPaths.push(...request.postDataJSON().paths);
    });
    const geometry = () =>
      dialog
        .locator('.pvi-root, .pvi-editor-badge, .pvi-status-dot-compact')
        .evaluateAll((elements) =>
          elements.map((element) => {
            const { x, y, width, height } = element.getBoundingClientRect();
            return { x, y, width, height };
          })
        );
    for (const width of [1280, 640]) {
      await page.setViewportSize({ width, height: 900 });
      await expect(dialog.locator('.pvi-status--ok')).toHaveCount(2);
      const before = await geometry();
      checkedPaths.length = 0;
      await expect(
        dialog.getByRole('button', { name: 'Insert variable', exact: true })
      ).toHaveCount(3);
      await expect(dialog.locator('.pvi-status-dot-compact')).toHaveCount(3);
      await toggle.check();
      await expect(saveEditor).toHaveText('');
      await expect(dialog.locator('.pvi-status-dot-compact').nth(1)).toHaveClass(
        /pvi-status--idle/
      );
      await expect(rootSelect).toBeVisible();
      expect(await geometry()).toEqual(before);
      const checked = page.waitForResponse(
        (response) =>
          response.url().endsWith('/check-paths') &&
          response.request().postDataJSON().paths.includes(`${root}/alternate.dat`)
      );
      await saveEditor.fill(`${root}/alternate.dat`);
      await checked;
      expect(checkedPaths).not.toContain(`${root}/other.dat`);
      await expect(rootSelect).toBeVisible();
      expect(await geometry()).toEqual(before);
      await toggle.uncheck();
      await expect(saveEditor).toHaveText('<root>/save.dat');
      await expect(rootSelect).toBeVisible();
      expect(await geometry()).toEqual(before);
    }
  } catch (error) {
    failed = true;
    throw error;
  } finally {
    await session.close(failed);
  }
});
