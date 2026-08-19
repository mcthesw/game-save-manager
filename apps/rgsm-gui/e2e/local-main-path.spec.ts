import { test, expect } from '@playwright/test';
import { readFile } from 'node:fs/promises';
import { DEVICE_A_ID } from './support/constants';
import { seedLocalConfig, writeSaveText } from './support/local-fixture';
import { startLocalSession } from './support/local-session';
import {
  expectLocalHead,
  expectSnapshotDates,
  localArchiveExists,
  snapshotMeta,
} from './support/local-assertions';
import {
  deleteSnapshotViaUi,
  getExtraBackups,
  getSettings,
  listSnapshotsFor,
} from './support/local-gui';
import { openGame } from './support/gui';
import { createRunRoot } from './support/rgsm-instance';
import { GAME_NAME } from './support/constants';

const CONTENT_V1 = 'progress level 1\n';
const CONTENT_V2 = 'progress level 2\n';
const CONTENT_LIVE = 'unsaved progress\n';

test('main path: create, apply latest, apply old snapshot, confirmations, delete', async ({
  browser,
}) => {
  const runRoot = await createRunRoot('local-main');
  const device = await seedLocalConfig(runRoot, {
    settings: {
      confirm_before_apply_latest: true,
      confirm_before_apply_snapshot: true,
    },
  });
  await writeSaveText(device.savePath, CONTENT_V1);
  const session = await startLocalSession(browser, { runRoot, device, label: 'local-main' });
  const { page, host } = session;
  let failed = false;
  try {
    await openGame(page);

    // One-click backup with empty description is the default entry.
    await page.getByRole('button', { name: 'Create new snapshot' }).click();
    await expect
      .poll(async () => (await listSnapshotsFor(host, GAME_NAME)).length, { timeout: 30_000 })
      .toBe(1);
    const firstId = (await listSnapshotsFor(host, GAME_NAME))[0]!.date;
    expect((await snapshotMeta(device.appDataDir, firstId)).describe).toBe('');
    await expectLocalHead(device.appDataDir, DEVICE_A_ID, firstId);

    // Second snapshot with a description, over newer save content.
    await writeSaveText(device.savePath, CONTENT_V2);
    await page.waitForTimeout(1100); // snapshot IDs are second-precision
    await page.getByPlaceholder('New backup description').fill('second');
    await page.getByRole('button', { name: 'Create new snapshot' }).click();
    await expect
      .poll(async () => (await listSnapshotsFor(host, GAME_NAME)).length, { timeout: 30_000 })
      .toBe(2);
    const secondId = (await listSnapshotsFor(host, GAME_NAME)).at(-1)!.date;
    expect((await snapshotMeta(device.appDataDir, secondId)).parent).toBe(firstId);

    // Cancelling the Apply latest confirmation must not touch anything.
    await writeSaveText(device.savePath, CONTENT_LIVE);
    await page.getByRole('button', { name: 'Apply latest' }).click();
    const confirmDialog = page.getByRole('dialog', { name: 'Warning' });
    await expect(confirmDialog.getByText('Confirm overwriting existing save?')).toBeVisible();
    await confirmDialog.getByRole('button', { name: 'Cancel' }).click();
    expect(await readFile(device.savePath, 'utf8')).toBe(CONTENT_LIVE);
    await expectLocalHead(device.appDataDir, DEVICE_A_ID, secondId);
    expect(await getExtraBackups(host, GAME_NAME)).toEqual([]);

    // Confirming Apply latest restores the newest snapshot.
    await page.getByRole('button', { name: 'Apply latest' }).click();
    await confirmDialog.getByRole('button', { name: 'Confirm' }).click();
    await expect
      .poll(async () => readFile(device.savePath, 'utf8'), { timeout: 30_000 })
      .toBe(CONTENT_V2);
    await expectLocalHead(device.appDataDir, DEVICE_A_ID, secondId);

    // In-row Apply on an older snapshot restores that exact content.
    const firstRow = page.getByRole('row').filter({ hasText: firstId });
    await firstRow.getByRole('button', { name: 'Apply' }).click();
    await confirmDialog.getByRole('button', { name: 'Cancel' }).click();
    expect(await readFile(device.savePath, 'utf8')).toBe(CONTENT_V2);

    await firstRow.getByRole('button', { name: 'Apply' }).click();
    await confirmDialog.getByRole('checkbox', { name: "Don't ask again" }).check();
    await confirmDialog.getByRole('button', { name: 'Confirm' }).click();
    await expect
      .poll(async () => readFile(device.savePath, 'utf8'), { timeout: 30_000 })
      .toBe(CONTENT_V1);
    await expectLocalHead(device.appDataDir, DEVICE_A_ID, firstId);

    // "Don't ask again" only disables the snapshot-entry confirmation; the
    // Apply latest entry keeps its own setting.
    await expect
      .poll(async () => (await getSettings(host)).confirm_before_apply_snapshot)
      .toBe(false);
    await expect.poll(async () => (await getSettings(host)).confirm_before_apply_latest).toBe(true);

    const secondRow = page.getByRole('row').filter({ hasText: secondId });
    await secondRow.getByRole('button', { name: 'Apply' }).click();
    await expect
      .poll(async () => readFile(device.savePath, 'utf8'), { timeout: 30_000 })
      .toBe(CONTENT_V2);

    await page.getByRole('button', { name: 'Apply latest' }).click();
    await expect(confirmDialog.getByText('Confirm overwriting existing save?')).toBeVisible();
    await confirmDialog.getByRole('button', { name: 'Cancel' }).click();

    // Deleting one snapshot keeps the other and the head intact.
    await deleteSnapshotViaUi(page, firstId);
    await expectSnapshotDates(device.appDataDir, [secondId]);
    expect(localArchiveExists(device.appDataDir, firstId)).toBe(false);
    expect(localArchiveExists(device.appDataDir, secondId)).toBe(true);
    await expectLocalHead(device.appDataDir, DEVICE_A_ID, secondId);
  } catch (error) {
    failed = true;
    throw error;
  } finally {
    await session.close(failed);
  }
});
