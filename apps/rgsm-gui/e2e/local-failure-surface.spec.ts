import { test, expect } from '@playwright/test';
import { readFile, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { DEVICE_A_ID, GAME_NAME } from './support/constants';
import { seedLocalConfig, writeSaveText } from './support/local-fixture';
import { startLocalSession } from './support/local-session';
import { archiveFileName, expectLocalHead, localSnapshotsDir } from './support/local-assertions';
import { createSnapshotForGame, listSnapshotsFor } from './support/local-gui';
import { expectActivity, openGame, snapshotRow } from './support/gui';
import { createRunRoot } from './support/rgsm-instance';

// Failure surface: a missing or tampered archive must abort the apply with a
// readable error and leave live files and the current position untouched; a
// missing save file must fail snapshot creation without writing metadata.
test('failure surface: missing archive, corrupted archive, missing save on create', async ({
  browser,
}) => {
  const runRoot = await createRunRoot('local-failure');
  const device = await seedLocalConfig(runRoot, {
    settings: { compute_archive_hash: true, verify_archive_before_apply: true },
  });
  await writeSaveText(device.savePath, 'v1\n');
  const session = await startLocalSession(browser, { runRoot, device, label: 'local-failure' });
  const { page, host } = session;
  let failed = false;
  try {
    const s1 = await createSnapshotForGame(host, GAME_NAME, 'first');
    await writeSaveText(device.savePath, 'v2\n');
    const s2 = await createSnapshotForGame(host, GAME_NAME, 'second');
    const archiveDir = localSnapshotsDir(device.appDataDir);
    const s1Archive = join(archiveDir, archiveFileName(s1));

    // Case A: archive deleted from disk -> apply fails, nothing changes.
    await rm(s1Archive);
    await openGame(page);
    await snapshotRow(page, s1).getByRole('button', { name: 'Apply' }).click();
    await expectActivity(page, 'Recovery failed');
    expect(await readFile(device.savePath, 'utf8')).toBe('v2\n');
    await expectLocalHead(device.appDataDir, DEVICE_A_ID, s2);

    // Case B: archive bytes tampered -> integrity gate aborts before restore.
    await writeFile(s1Archive, 'not-a-real-archive');
    await snapshotRow(page, s1).getByRole('button', { name: 'Apply' }).click();
    const integrityDialog = page.getByRole('dialog', { name: 'Archive Corrupted' });
    await expect(integrityDialog).toBeVisible({ timeout: 15_000 });
    await integrityDialog.getByRole('button', { name: 'Confirm' }).click();
    expect(await readFile(device.savePath, 'utf8')).toBe('v2\n');
    await expectLocalHead(device.appDataDir, DEVICE_A_ID, s2);

    // Case C: save file missing -> snapshot creation fails, metadata untouched.
    await rm(device.savePath);
    await page.getByRole('button', { name: 'Create new snapshot' }).click();
    await expectActivity(page, 'Backup failed');
    expect((await listSnapshotsFor(host, GAME_NAME)).length).toBe(2);

    // Restoring the file makes creation work again.
    await writeSaveText(device.savePath, 'v3\n');
    await page.waitForTimeout(1200);
    await page.getByRole('button', { name: 'Create new snapshot' }).click();
    await expect
      .poll(async () => (await listSnapshotsFor(host, GAME_NAME)).length, { timeout: 30_000 })
      .toBe(3);
  } catch (error) {
    failed = true;
    throw error;
  } finally {
    await session.close(failed);
  }
});
