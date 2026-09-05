import assert from 'node:assert/strict';
import { test, expect } from '@playwright/test';
import { chmod, readFile, rm, stat } from 'node:fs/promises';
import { join } from 'node:path';
import { DEVICE_A_ID, GAME_NAME } from './support/constants';
import { seedLocalConfig, writeSaveText } from './support/local-fixture';
import { startLocalSession } from './support/local-session';
import {
  archiveFileName,
  expectLocalHead,
  expectSnapshotDates,
  expectSnapshotParent,
  localArchiveExists,
  localSnapshotsDir,
} from './support/local-assertions';
import { createSnapshotForGame, deleteSnapshotViaUi } from './support/local-gui';
import { openGame } from './support/gui';
import { createRunRoot } from './support/rgsm-instance';

for (const denyCatalogWrite of [false, true]) {
  test(`delete snapshot: ${denyCatalogWrite ? 'retry a denied catalog write on Windows' : 'remove a missing archive record'}`, async ({
    browser,
  }) => {
    test.skip(denyCatalogWrite && process.platform !== 'win32', 'Windows read-only replacement');
    const runRoot = await createRunRoot('local-delete-recovery');
    const device = await seedLocalConfig(runRoot);
    await writeSaveText(device.savePath, 'live save\n');
    const session = await startLocalSession(browser, { runRoot, device, label: 'delete-recovery' });
    let failed = false;
    try {
      const parent = await createSnapshotForGame(session.host, GAME_NAME, 'parent');
      const child = await createSnapshotForGame(session.host, GAME_NAME, 'child');
      await openGame(session.page);
      const snapshotsDir = localSnapshotsDir(device.appDataDir);
      if (denyCatalogWrite) {
        const catalog = join(snapshotsDir, 'Backups.json');
        const mode = (await stat(catalog)).mode;
        await chmod(catalog, 0o444);
        try {
          await assert.rejects(
            deleteSnapshotViaUi(session.page, child),
            /delete-snapshot failed \(400\)/
          );
        } finally {
          await chmod(catalog, mode);
        }
        expect(localArchiveExists(device.appDataDir, child)).toBe(false);
        await expectSnapshotDates(device.appDataDir, [parent, child]);
        await expectLocalHead(device.appDataDir, DEVICE_A_ID, child);
      } else {
        await rm(join(snapshotsDir, archiveFileName(child)));
      }

      await deleteSnapshotViaUi(session.page, child);

      await expectSnapshotDates(device.appDataDir, [parent]);
      await expectLocalHead(device.appDataDir, DEVICE_A_ID, parent);
      await expectSnapshotParent(device.appDataDir, parent, null);
      expect(localArchiveExists(device.appDataDir, parent)).toBe(true);
      expect(await readFile(device.savePath, 'utf8')).toBe('live save\n');
    } catch (error) {
      failed = true;
      throw error;
    } finally {
      await session.close(failed);
    }
  });
}
