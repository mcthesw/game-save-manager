import { test, expect } from '@playwright/test';
import { DEVICE_A_ID, GAME_NAME } from './support/constants';
import { seedLocalConfig, writeSaveText } from './support/local-fixture';
import { startLocalSession } from './support/local-session';
import {
  expectLocalHead,
  expectSnapshotDates,
  expectSnapshotParent,
  localArchiveExists,
} from './support/local-assertions';
import { createSnapshotForGame } from './support/local-gui';
import { openGame, snapshotRow } from './support/gui';
import { createRunRoot } from './support/rgsm-instance';

test('batch delete removes selected snapshots and rewires the tree', async ({ browser }) => {
  const runRoot = await createRunRoot('local-batch');
  const device = await seedLocalConfig(runRoot);
  const session = await startLocalSession(browser, { runRoot, device, label: 'local-batch' });
  const { page, host } = session;
  let failed = false;
  try {
    const ids: string[] = [];
    for (const [index, describe] of ['s1', 's2', 's3', 's4'].entries()) {
      await writeSaveText(device.savePath, `v${index + 1}\n`);
      ids.push(await createSnapshotForGame(host, GAME_NAME, describe));
    }
    const [s1, s2, s3, s4] = ids as [string, string, string, string];
    await expectLocalHead(device.appDataDir, DEVICE_A_ID, s4);

    // Delete the two middle links: s4 must re-parent to s1, head stays.
    await openGame(page);
    await snapshotRow(page, s2).getByRole('checkbox').click();
    await snapshotRow(page, s3).getByRole('checkbox').click();
    await page.getByRole('button', { name: 'Batch delete' }).click();
    const prompt = page.getByRole('dialog');
    await expect(prompt.getByText(/enter yes to confirm/)).toBeVisible();
    await prompt.getByRole('textbox').fill('yes');
    await prompt.getByRole('button', { name: 'Confirm' }).click();

    await expectSnapshotDates(device.appDataDir, [s1, s4]);
    await expectSnapshotParent(device.appDataDir, s4, s1);
    await expectLocalHead(device.appDataDir, DEVICE_A_ID, s4);
    expect(localArchiveExists(device.appDataDir, s2)).toBe(false);
    expect(localArchiveExists(device.appDataDir, s3)).toBe(false);
    expect(localArchiveExists(device.appDataDir, s1)).toBe(true);
    expect(localArchiveExists(device.appDataDir, s4)).toBe(true);

    // Delete the head and its parent: position lands on the nearest surviving
    // ancestor.
    await writeSaveText(device.savePath, 'v5\n');
    const s5 = await createSnapshotForGame(host, GAME_NAME, 's5');
    await expectSnapshotParent(device.appDataDir, s5, s4);
    await page.reload();
    await openGame(page);
    await snapshotRow(page, s4).getByRole('checkbox').click();
    await snapshotRow(page, s5).getByRole('checkbox').click();
    await page.getByRole('button', { name: 'Batch delete' }).click();
    await expect(prompt.getByText(/enter yes to confirm/)).toBeVisible();
    await prompt.getByRole('textbox').fill('yes');
    await prompt.getByRole('button', { name: 'Confirm' }).click();

    await expectSnapshotDates(device.appDataDir, [s1]);
    await expectLocalHead(device.appDataDir, DEVICE_A_ID, s1);
    expect(localArchiveExists(device.appDataDir, s1)).toBe(true);
  } catch (error) {
    failed = true;
    throw error;
  } finally {
    await session.close(failed);
  }
});
