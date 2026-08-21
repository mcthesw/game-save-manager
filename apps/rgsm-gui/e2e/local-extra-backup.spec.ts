import { test, expect } from '@playwright/test';
import { readFile } from 'node:fs/promises';
import { DEVICE_A_ID, GAME_NAME } from './support/constants';
import { seedLocalConfig, writeSaveText } from './support/local-fixture';
import { startLocalSession } from './support/local-session';
import { expectLocalHead, readBackupsJson } from './support/local-assertions';
import {
  createSnapshotForGame,
  getExtraBackups,
  listSnapshotsFor,
  updateSettings,
} from './support/local-gui';
import { openGame, snapshotRow } from './support/gui';
import { createRunRoot } from './support/rgsm-instance';

test('extra backups: created on apply, undo restores content and head, retention trims', async ({
  browser,
}) => {
  const runRoot = await createRunRoot('local-extra');
  const device = await seedLocalConfig(runRoot, {
    settings: { max_extra_backup_count: 2 },
  });
  await writeSaveText(device.savePath, 'v1\n');
  const session = await startLocalSession(browser, { runRoot, device, label: 'local-extra' });
  const { page, host } = session;
  let failed = false;
  try {
    const firstId = await createSnapshotForGame(host, GAME_NAME, 'first');
    await writeSaveText(device.savePath, 'v2\n');
    const secondId = await createSnapshotForGame(host, GAME_NAME, 'second');

    // Applying with unsaved progress creates a safety extra backup first.
    await writeSaveText(device.savePath, 'v3-unsaved\n');
    await openGame(page);
    await snapshotRow(page, firstId).getByRole('button', { name: 'Apply' }).click();
    await expect
      .poll(async () => readFile(device.savePath, 'utf8'), { timeout: 30_000 })
      .toBe('v1\n');
    const extrasAfterApply = await getExtraBackups(host, GAME_NAME);
    expect(extrasAfterApply.length).toBe(1);
    await expectLocalHead(device.appDataDir, DEVICE_A_ID, firstId);

    // Undo restores the overwritten progress and moves the position back.
    await page.getByRole('button', { name: 'Undo last apply' }).click();
    const undoDialog = page.getByRole('dialog', { name: 'Warning' });
    await undoDialog.getByRole('button', { name: 'Confirm' }).click();
    await expect
      .poll(async () => readFile(device.savePath, 'utf8'), { timeout: 30_000 })
      .toBe('v3-unsaved\n');
    await expect
      .poll(
        async () => (await readBackupsJson(device.appDataDir)).device_heads[DEVICE_A_ID] ?? null,
        { timeout: 15_000 }
      )
      .toBe(secondId);

    // Restoring an extra backup from the drawer changes files only: no new
    // snapshot, position untouched.
    await writeSaveText(device.savePath, 'v4-unsaved\n');
    await page.getByRole('button', { name: 'More actions' }).click();
    await page.getByRole('menuitem', { name: 'Extra backups' }).click();
    const drawer = page.getByRole('dialog', { name: 'Extra backups' });
    await expect(drawer).toBeVisible();
    await drawer.getByRole('button', { name: 'Apply' }).first().click();
    await expect
      .poll(async () => readFile(device.savePath, 'utf8'), { timeout: 30_000 })
      .toBe('v3-unsaved\n');
    expect((await listSnapshotsFor(host, GAME_NAME)).length).toBe(2);
    await expectLocalHead(device.appDataDir, DEVICE_A_ID, secondId);
    await page.keyboard.press('Escape');
    await expect(drawer).toBeHidden({ timeout: 10_000 });

    // Retention keeps only the newest two extras.
    for (const target of [secondId, firstId, secondId]) {
      await snapshotRow(page, target).getByRole('button', { name: 'Apply' }).click();
      await expect
        .poll(async () => readFile(device.savePath, 'utf8'), { timeout: 30_000 })
        .toBe(target === firstId ? 'v1\n' : 'v2\n');
    }
    await expect
      .poll(async () => (await getExtraBackups(host, GAME_NAME)).length, { timeout: 15_000 })
      .toBe(2);

    // With the setting off, applies create no extras and undo stays
    // unavailable with an explanatory tooltip.
    await updateSettings(host, { extra_backup_when_apply: false });
    await page.reload();
    await openGame(page);
    await snapshotRow(page, secondId).getByRole('button', { name: 'Apply' }).click();
    await expect
      .poll(async () => readFile(device.savePath, 'utf8'), { timeout: 30_000 })
      .toBe('v2\n');
    expect((await getExtraBackups(host, GAME_NAME)).length).toBe(2);
    const undo = page.getByRole('button', {
      name: 'Enable "Extra backup before apply" in Settings to use the undo feature',
    });
    await expect(undo).toBeVisible();
    await expect(undo).toBeDisabled();
  } catch (error) {
    failed = true;
    throw error;
  } finally {
    await session.close(failed);
  }
});

test('extra backups: deleting one from the drawer', async ({ browser }) => {
  const runRoot = await createRunRoot('local-extra-del');
  const device = await seedLocalConfig(runRoot);
  await writeSaveText(device.savePath, 'v1\n');
  const session = await startLocalSession(browser, { runRoot, device, label: 'local-extra-del' });
  const { page, host } = session;
  let failed = false;
  try {
    const firstId = await createSnapshotForGame(host, GAME_NAME, 'first');
    await writeSaveText(device.savePath, 'v2-live\n');
    await openGame(page);
    await snapshotRow(page, firstId).getByRole('button', { name: 'Apply' }).click();
    await expect
      .poll(async () => readFile(device.savePath, 'utf8'), { timeout: 30_000 })
      .toBe('v1\n');
    expect((await getExtraBackups(host, GAME_NAME)).length).toBe(1);

    await page.getByRole('button', { name: 'More actions' }).click();
    await page.getByRole('menuitem', { name: 'Extra backups' }).click();
    const drawer = page.getByRole('dialog', { name: 'Extra backups' });
    await expect(drawer).toBeVisible();
    await drawer.getByRole('button', { name: 'Delete' }).first().click();
    await page.getByRole('dialog').getByRole('button', { name: 'Confirm' }).click();
    await expect
      .poll(async () => (await getExtraBackups(host, GAME_NAME)).length, { timeout: 15_000 })
      .toBe(0);
  } catch (error) {
    failed = true;
    throw error;
  } finally {
    await session.close(failed);
  }
});
