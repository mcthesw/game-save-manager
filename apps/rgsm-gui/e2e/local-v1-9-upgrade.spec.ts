import { test, expect, type BrowserContext } from '@playwright/test';
import { access, mkdir, readFile, readdir, writeFile } from 'node:fs/promises';
import { dirname, join } from 'node:path';
import { DEVICE_A_ID, GAME_NAME, deviceProfileFileName } from './support/constants';
import { openApp, openGame, snapshotRow } from './support/gui';
import {
  applySnapshotViaApi,
  createSnapshotForGame,
  getLocalGame,
  listSnapshotsFor,
} from './support/local-gui';
import {
  createRunRoot,
  hostPost,
  newDeviceContext,
  removeRunRoot,
  startRgsmHost,
  workspacePath,
  type RgsmHost,
} from './support/rgsm-instance';

test('1.9 owned configuration and V4/V5 archives survive startup, local conversion and restart', async ({
  browser,
}, testInfo) => {
  const runRoot = await createRunRoot('local-v1-9-upgrade');
  const appDataDir = join(runRoot, 'app-data');
  const archiveRoot = join(appDataDir, 'save_data');
  const gameRoot = join(runRoot, '安装目录');
  const archiveDir = join(archiveRoot, 'beta-game');
  const configDir = join(appDataDir, 'GameSaveManager.config.v2');
  const fixtureDir = workspacePath('apps/rgsm-gui/e2e/fixtures/local-v1-9-upgrade');
  const owners = JSON.parse(
    (await readFile(join(fixtureDir, 'owners.json'), 'utf8'))
      .replaceAll('__BACKUP_PATH__', archiveRoot.replaceAll('\\', '/'))
      .replaceAll('__INSTALL_PATH__', gameRoot.replaceAll('\\', '/'))
  );
  await mkdir(join(configDir, 'device-profiles'), { recursive: true });
  await mkdir(archiveDir, { recursive: true });
  for (const [name, value] of [
    ['shared-library.json', owners.shared_library],
    ['local-state.json', owners.local_state],
    [join('device-profiles', deviceProfileFileName(DEVICE_A_ID)), owners.device_profile],
  ] as const) {
    await writeFile(join(configDir, name), JSON.stringify(value));
  }
  const savePaths = ['primary', 'secondary'].map((folder) => join(gameRoot, folder, 'profile.sav'));
  const writeLive = async (text: string) => {
    for (const path of savePaths) {
      await mkdir(dirname(path), { recursive: true });
      await writeFile(path, text);
    }
  };
  const expectLive = async (values: string[]) => {
    expect(await Promise.all(savePaths.map((path) => readFile(path, 'utf8')))).toEqual(values);
  };
  await writeLive('live-before-upgrade');
  const ids = ['8b4c7323-4c44-4e95-b7d9-4ce72bfca401', 'f4b37f3d-8e8c-40b1-a167-2a4df9b2d502'];
  const originals = await Promise.all(
    [4, 5].map((version) => readFile(join(fixtureDir, `archive-v${version}.7z`)))
  );
  const backups = [];
  for (const [index, id] of ids.entries()) {
    const path = join(archiveDir, `${id}.7z`);
    await writeFile(path, originals[index]!);
    backups.push({
      date: id,
      describe: `Beta V${index + 4}`,
      path,
      archive_format: 'seven_z',
      size: originals[index]!.length,
      parent: index ? ids[0] : null,
      device_id: DEVICE_A_ID,
      created_at: Date.UTC(2026, 0, index + 1),
      created_by: 'Manual',
    });
  }
  const catalogPath = join(archiveDir, 'Backups.json');
  await writeFile(
    catalogPath,
    JSON.stringify({
      name: GAME_NAME,
      backups,
      device_heads: { [DEVICE_A_ID]: ids[1] },
      sync_version: 3,
    })
  );

  let host: RgsmHost | undefined;
  let context: BrowserContext | undefined;
  let failed = false;
  const start = () =>
    startRgsmHost({ appDataDir, deviceId: DEVICE_A_ID, logPath: join(runRoot, 'host.log') });
  try {
    host = await start();
    const game = await getLocalGame(host, GAME_NAME);
    expect(game.storage_key).toBe('beta-game');
    expect(game.save_paths.map((unit) => unit.id)).toEqual([11, 12]);
    expect(game.device_bindings).toMatchObject({
      [DEVICE_A_ID]: { installationPath: gameRoot.replaceAll('\\', '/') },
    });
    const config = await hostPost<{
      favorites: Array<{ game_id: string }>;
      quick_action: { quick_action_game_id: string };
    }>(host, '/api/v1/get-local-config');
    expect(config.data.favorites[0]?.game_id).toBe('beta-game');
    expect(config.data.quick_action.quick_action_game_id).toBe('beta-game');
    const preservedConfig = join(
      appDataDir,
      'GameSaveManager.installations-before-upgrade.json.bak'
    );
    const preservedBytes = await readFile(preservedConfig);

    ({ context } = await newDeviceContext(browser, host));
    let page = context.pages()[0]!;
    await openApp(page);
    await openGame(page);
    for (const [index, id] of ids.entries()) {
      await expect(snapshotRow(page, id)).toContainText(`Beta V${index + 4}`);
      await writeLive('before-original-restore');
      await snapshotRow(page, id).getByRole('button', { name: 'Apply', exact: true }).click();
      await expect.poll(async () => readFile(savePaths[0]!, 'utf8')).toBe('primary-beta');
      await expectLive(['primary-beta', 'secondary-beta']);
      expect(await readFile(backups[index]!.path)).toEqual(originals[index]);
    }
    await writeLive('new-progress');
    const createdId = await createSnapshotForGame(host, GAME_NAME, 'Created after beta upgrade');
    const before = await listSnapshotsFor(host, GAME_NAME);
    const created = before.find((snapshot) => snapshot.date === createdId)!;
    await testInfo.attach('same-name-backup.7z', {
      path: created.path,
      contentType: 'application/x-7z-compressed',
    });
    expect(created.parent).toBe(ids[1]);
    expect(created.path).toMatch(/\d{4}-\d{2}-\d{2}_\d{2}-\d{2}-\d{2}_[0-9a-f]{12}\.7z$/);

    await page.goto('/Settings');
    await page.getByRole('button', { name: 'Backup settings', exact: true }).click();
    const panel = page.getByTestId('local-upgrade');
    await expect(panel.getByText('2 older backups', { exact: true })).toBeVisible();
    await panel.getByRole('button', { name: 'Start upgrade', exact: true }).click();
    await expect(panel.getByText('2 of 2 backups upgraded', { exact: true })).toBeVisible();
    await expectLive(['new-progress', 'new-progress']);
    const retained = join(
      archiveRoot,
      '.rgsm-upgrade',
      'originals',
      Buffer.from('beta-game').toString('hex')
    );
    const retainedNames = await readdir(retained);
    expect(retainedNames).toHaveLength(2);
    for (const [index, id] of ids.entries()) {
      const name = retainedNames.find((name) => name.endsWith(`_${id}.7z`));
      expect(name).toBeTruthy();
      expect(await readFile(join(retained, name!))).toEqual(originals[index]);
    }

    await context.close();
    context = undefined;
    await host.stop();
    host = await start();
    expect(await readFile(preservedConfig)).toEqual(preservedBytes);
    const catalog = JSON.parse(await readFile(catalogPath, 'utf8'));
    expect(catalog.backups.map((snapshot: { describe: string }) => snapshot.describe)).toEqual([
      'Beta V4',
      'Beta V5',
      'Created after beta upgrade',
    ]);
    expect(catalog.backups.map((snapshot: { date: string }) => snapshot.date)).toEqual([
      ...ids,
      createdId,
    ]);
    expect(catalog.device_heads[DEVICE_A_ID]).toBe(createdId);
    expect(catalog.backups.map((snapshot: { parent: string | null }) => snapshot.parent)).toEqual([
      null,
      ids[0],
      ids[1],
    ]);
    for (const id of [...ids, createdId]) {
      await writeLive('before-restarted-restore');
      await applySnapshotViaApi(host, GAME_NAME, id);
      await expectLive(
        id === createdId ? ['new-progress', 'new-progress'] : ['primary-beta', 'secondary-beta']
      );
    }
    ({ context } = await newDeviceContext(browser, host));
    page = context.pages()[0]!;
    await openApp(page);
    await openGame(page);
    for (const id of [...ids, createdId]) await expect(snapshotRow(page, id)).toBeVisible();
    await expect(access(join(archiveRoot, '.rgsm-upgrade', 'originals'))).resolves.toBeUndefined();
  } catch (error) {
    failed = true;
    throw error;
  } finally {
    await context?.close();
    await host?.stop();
    if (!failed) await removeRunRoot(runRoot);
  }
});
