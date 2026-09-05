import { test, expect, type BrowserContext } from '@playwright/test';
import { access, readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { DEVICE_A_ID, GAME_NAME, PARENT_SNAPSHOT_ID, CHILD_SNAPSHOT_ID } from './support/constants';
import { openApp, openGame, snapshotRow } from './support/gui';
import { getLocalGame, getSettings, getExtraBackups, listSnapshotsFor } from './support/local-gui';
import { waitForCommand } from './support/command-result';
import { seedReleasedUpgrade } from './support/released-upgrade';
import {
  createRunRoot,
  hostPost,
  newDeviceContext,
  removeRunRoot,
  startRgsmHost,
  startTestWeb,
  type RgsmHost,
} from './support/rgsm-instance';

for (const version of ['1.7.0', '1.8.0'] as const) {
  test(`${version} concrete paths upgrade without configuration repair and retain restore semantics`, async ({
    browser,
  }) => {
    const runRoot = await createRunRoot(`released-${version}`);
    const scene = await seedReleasedUpgrade(runRoot, version);
    const web = await startTestWeb();
    let host: RgsmHost | undefined;
    let context: BrowserContext | undefined;
    let failed = false;
    const readCatalog = async () => {
      const game = await getLocalGame(host!, GAME_NAME);
      const result = await hostPost<{ device_heads: Record<string, string> }>(
        host!,
        '/api/v1/get-game-snapshots-info',
        { game }
      );
      expect(result.ok, result.raw).toBe(true);
      return result.data;
    };
    try {
      host = await startRgsmHost({
        appDataDir: scene.appDataDir,
        deviceId: DEVICE_A_ID,
        logPath: join(runRoot, 'host.log'),
      });
      const game = await getLocalGame(host, GAME_NAME);
      expect(game.save_paths.map((unit) => unit.id)).toEqual(scene.ids);
      expect(game.save_paths.map((unit) => unit.source)).toEqual([
        expect.objectContaining({ type: 'concrete', unit_type: 'Folder' }),
        expect.objectContaining({ type: 'concrete', unit_type: 'File' }),
        expect.objectContaining({ type: 'concrete', unit_type: 'Folder' }),
      ]);
      expect((await getSettings(host)).extra_backup_when_apply).toBe(true);
      expect((await readCatalog()).device_heads[DEVICE_A_ID]).toBe(PARENT_SNAPSHOT_ID);
      expect(
        (await listSnapshotsFor(host, GAME_NAME)).find((s) => s.date === CHILD_SNAPSHOT_ID)?.parent
      ).toBe(PARENT_SNAPSHOT_ID);
      ({ context } = await newDeviceContext(browser, host));
      let page = context.pages()[0]!;
      await openApp(page);
      await openGame(page);
      await page.getByPlaceholder('New backup description').fill('Created after upgrade');
      await page.getByRole('button', { name: 'Create new snapshot' }).click();
      await expect.poll(async () => (await listSnapshotsFor(host!, GAME_NAME)).length).toBe(3);
      const created = (await listSnapshotsFor(host, GAME_NAME)).find(
        (s) => s.describe === 'Created after upgrade'
      )!;
      expect(created.parent).toBe(PARENT_SNAPSHOT_ID);

      await snapshotRow(page, PARENT_SNAPSHOT_ID).getByRole('button', { name: 'Apply' }).click();
      await waitForCommand(page, '/api/v1/restore-snapshot', PARENT_SNAPSHOT_ID, () =>
        page
          .getByRole('dialog', { name: 'Warning' })
          .getByRole('button', { name: 'Confirm' })
          .click()
      );
      for (const path of scene.savePaths) {
        expect(await readFile(path, 'utf8')).toBe(`saved-${PARENT_SNAPSHOT_ID}`);
      }
      expect(await readFile(join(scene.gameRoot, 'Saves', 'new-only.txt'), 'utf8')).toBe(
        'keep-unless-replaced'
      );
      await expect(access(join(scene.gameRoot, 'Replaced', 'new-only.txt'))).rejects.toThrow();
      expect(await getExtraBackups(host, GAME_NAME)).toHaveLength(1);

      await context.close();
      context = undefined;
      await host.stop();
      host = await startRgsmHost({
        appDataDir: scene.appDataDir,
        deviceId: DEVICE_A_ID,
        logPath: join(runRoot, 'restart.log'),
      });
      ({ context } = await newDeviceContext(browser, host));
      page = context.pages()[0]!;
      await openApp(page);
      await openGame(page);
      await expect(snapshotRow(page, created.date)).toBeVisible();
      expect((await readCatalog()).device_heads[DEVICE_A_ID]).toBe(PARENT_SNAPSHOT_ID);
      for (const [path, bytes] of scene.originalArchives)
        expect(await readFile(path)).toEqual(bytes);
    } catch (error) {
      failed = true;
      throw error;
    } finally {
      await context?.close();
      await host?.stop();
      await web.stop();
      if (!failed) await removeRunRoot(runRoot);
    }
  });
}
