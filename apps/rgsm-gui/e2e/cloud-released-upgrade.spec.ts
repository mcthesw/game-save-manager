import { test, expect } from '@playwright/test';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { DEVICE_A_ID, GAME_NAME, PARENT_SNAPSHOT_ID } from './support/constants';
import { seedReleasedUpgrade } from './support/released-upgrade';
import {
  confirmCutover,
  expectCutoverSuccess,
  openApp,
  openGame,
  openSyncSettings,
  snapshotRow,
} from './support/gui';
import { waitForCommand } from './support/command-result';
import { getLocalGame } from './support/local-gui';
import {
  createRunRoot,
  newDeviceContext,
  removeRunRoot,
  startRgsmHost,
  type RgsmHost,
} from './support/rgsm-instance';

for (const version of ['1.7.0', '1.8.0'] as const) {
  test(`released ${version} cloud configuration upgrades and restores without manual repair`, async ({
    browser,
  }) => {
    const runRoot = await createRunRoot(`cloud-released-${version}`);
    const scene = await seedReleasedUpgrade(runRoot, version);
    const configPath = join(scene.appDataDir, 'GameSaveManager.config.json');
    const config = JSON.parse(await readFile(configPath, 'utf8'));
    config.games = config.games.slice(0, 1);
    config.quick_action.quick_action_game = null;
    const cloudRoot = join(runRoot, 'cloud');
    config.settings.cloud_settings.backend = { type: 'Fs' };
    config.settings.cloud_settings.root_path = cloudRoot;
    config.settings.cloud_settings.auto_sync_interval = 0;
    const cloudArchiveRoot = join(cloudRoot, 'save_data', GAME_NAME);
    await mkdir(cloudArchiveRoot, { recursive: true });
    const originalConfig = Buffer.from(JSON.stringify(config));
    await writeFile(configPath, originalConfig);
    await writeFile(join(cloudRoot, 'GameSaveManager.config.json'), originalConfig);
    const originalCatalog = await readFile(join(scene.archiveDir, 'Backups.json'));
    await writeFile(join(cloudArchiveRoot, 'Backups.json'), originalCatalog);
    for (const [archive, bytes] of scene.originalArchives) {
      await writeFile(join(cloudArchiveRoot, archive.split(/[\\/]/).at(-1)!), bytes);
    }
    expect(config.games[0]).not.toHaveProperty('storage_key');
    expect(config.version).toBe(version);
    let host: RgsmHost | undefined;
    let context;
    let failed = false;
    try {
      host = await startRgsmHost({
        appDataDir: scene.appDataDir,
        deviceId: DEVICE_A_ID,
        logPath: join(runRoot, 'host.log'),
      });
      const started = await newDeviceContext(browser, host);
      context = started.context;
      const page = started.page;
      await openApp(page);
      await openSyncSettings(page);
      await confirmCutover(page);
      await expectCutoverSuccess(page);
      const game = await getLocalGame(host, GAME_NAME);
      expect(game.storage_key).toBe(GAME_NAME);
      expect(game.save_paths.map((unit) => unit.id)).toEqual(scene.ids);
      await openGame(page);
      await snapshotRow(page, PARENT_SNAPSHOT_ID).getByRole('button', { name: 'Apply' }).click();
      await waitForCommand(page, '/api/v1/restore-snapshot', PARENT_SNAPSHOT_ID, () =>
        page
          .getByRole('dialog', { name: 'Warning' })
          .getByRole('button', { name: 'Confirm' })
          .click()
      );
      for (const path of scene.savePaths)
        expect(await readFile(path, 'utf8')).toBe(`saved-${PARENT_SNAPSHOT_ID}`);
      expect(await readFile(join(cloudRoot, 'GameSaveManager.config.json'))).toEqual(
        originalConfig
      );
      expect(await readFile(join(cloudArchiveRoot, 'Backups.json'))).toEqual(originalCatalog);
      for (const [archive, bytes] of scene.originalArchives) {
        const filename = archive.split(/[\\/]/).at(-1)!;
        expect(await readFile(join(cloudArchiveRoot, filename))).toEqual(bytes);
        expect(await readFile(join(cloudRoot, 'v2', 'archives', GAME_NAME, filename))).toEqual(
          bytes
        );
      }
    } catch (error) {
      failed = true;
      throw error;
    } finally {
      await context?.close();
      await host?.stop();
      if (!failed) await removeRunRoot(runRoot);
    }
  });
}
