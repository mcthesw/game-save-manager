import { test, expect } from '@playwright/test';
import { copyFile, mkdir, readFile, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { DEVICE_A_ID, GAME_NAME } from './support/constants';
import { seedLocalConfig, writeSaveText } from './support/local-fixture';
import { startLocalSession } from './support/local-session';
import {
  expectLocalHead,
  expectSnapshotParent,
  localSnapshotsDir,
  snapshotMeta,
} from './support/local-assertions';
import { listSnapshotsFor } from './support/local-gui';
import { openGame, snapshotRow } from './support/gui';
import { createRunRoot } from './support/rgsm-instance';

const LEGACY_DATE = '2020-01-02_03-04-05';
const LEGACY_CONTENT = 'legacy-v1-content\n';
// One stored-entry zip with `save.dat` at the archive root (pre-index-prefix
// layout, no archive comment). Generated deterministically; see repo history.
const LEGACY_ZIP_FIXTURE = join(import.meta.dirname, 'fixtures', 'legacy-local', 'snapshot.zip');

// A backup made by an old app version (flat zip, legacy `head` field) must
// stay visible and restorable, and new snapshots must chain onto it.
test('legacy zip backup from an old version restores and joins the tree', async ({ browser }) => {
  const runRoot = await createRunRoot('local-legacy');
  const device = await seedLocalConfig(runRoot);
  const backupDir = localSnapshotsDir(device.appDataDir);
  await mkdir(backupDir, { recursive: true });
  await copyFile(LEGACY_ZIP_FIXTURE, join(backupDir, `${LEGACY_DATE}.zip`));
  await writeFile(
    join(backupDir, 'Backups.json'),
    JSON.stringify({
      name: GAME_NAME,
      backups: [
        {
          date: LEGACY_DATE,
          describe: 'legacy snapshot',
          path: '',
          size: 0,
          parent: null,
        },
      ],
      head: LEGACY_DATE,
    })
  );
  await writeSaveText(device.savePath, 'current\n');

  const session = await startLocalSession(browser, { runRoot, device, label: 'local-legacy' });
  const { page, host } = session;
  let failed = false;
  try {
    await openGame(page);
    const row = snapshotRow(page, LEGACY_DATE);
    await expect(row.getByText('legacy snapshot')).toBeVisible();

    // Applying the old archive restores its content and moves the position.
    await row.getByRole('button', { name: 'Apply' }).click();
    await expect
      .poll(async () => readFile(device.savePath, 'utf8'), { timeout: 30_000 })
      .toBe(LEGACY_CONTENT);
    await expectLocalHead(device.appDataDir, DEVICE_A_ID, LEGACY_DATE);
    expect((await snapshotMeta(device.appDataDir, LEGACY_DATE)).archive_format).toBe('zip');

    // A new snapshot chains onto the legacy node.
    await writeSaveText(device.savePath, 'current-v2\n');
    await page.waitForTimeout(1200);
    await page
      .getByRole('textbox', { name: 'New backup description', exact: true })
      .fill('after legacy');
    await page.getByRole('button', { name: 'Create new snapshot' }).click();
    await expect
      .poll(async () => (await listSnapshotsFor(host, GAME_NAME)).length, { timeout: 30_000 })
      .toBe(2);
    const newer = (await listSnapshotsFor(host, GAME_NAME)).find(
      (item) => item.describe === 'after legacy'
    )!;
    await expectSnapshotParent(device.appDataDir, newer.date, LEGACY_DATE);
  } catch (error) {
    failed = true;
    throw error;
  } finally {
    await session.close(failed);
  }
});
