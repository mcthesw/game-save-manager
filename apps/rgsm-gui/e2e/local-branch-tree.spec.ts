import { test, expect, type Page } from '@playwright/test';
import { readFile } from 'node:fs/promises';
import { DEVICE_A_ID, GAME_NAME } from './support/constants';
import { seedLocalConfig, writeSaveText } from './support/local-fixture';
import { startLocalSession } from './support/local-session';
import {
  expectLocalHead,
  expectSnapshotDates,
  expectSnapshotParent,
  localArchiveExists,
  readBackupsJson,
} from './support/local-assertions';
import {
  applySnapshotViaApi,
  confirmSnapshotDeletion,
  createSnapshotForGame,
} from './support/local-gui';
import { openGame } from './support/gui';
import { createRunRoot } from './support/rgsm-instance';

function branchNode(page: Page, snapshotId: string) {
  return page.locator(`.vue-flow__node[data-id="${snapshotId}"]`);
}

async function branchNodeAction(page: Page, snapshotId: string, action: string): Promise<void> {
  // Clicking a node whose popover is already open toggles it closed, so
  // dismiss any leftover popover with a pane click before opening fresh.
  await page.locator('.vue-flow__pane').click({ position: { x: 8, y: 8 }, timeout: 10_000 });
  await branchNode(page, snapshotId).click({ timeout: 10_000 });
  await page.getByRole('button', { name: action, exact: true }).click({ timeout: 10_000 });
}

test('snapshot tree: branch on apply-then-create, set head, delete head fallbacks', async ({
  browser,
}) => {
  const runRoot = await createRunRoot('local-branch');
  const device = await seedLocalConfig(runRoot);
  await writeSaveText(device.savePath, 'v1\n');
  const session = await startLocalSession(browser, { runRoot, device, label: 'local-branch' });
  const { page, host } = session;
  let failed = false;
  try {
    const firstId = await createSnapshotForGame(host, GAME_NAME, 'first');
    await writeSaveText(device.savePath, 'v2\n');
    const secondId = await createSnapshotForGame(host, GAME_NAME, 'second');
    await expectSnapshotParent(device.appDataDir, secondId, firstId);

    // Applying an older snapshot then creating a new one branches off it.
    await applySnapshotViaApi(host, GAME_NAME, firstId);
    await expectLocalHead(device.appDataDir, DEVICE_A_ID, firstId);
    await writeSaveText(device.savePath, 'v1-branched\n');
    const branchedId = await createSnapshotForGame(host, GAME_NAME, 'branched');
    await expectSnapshotParent(device.appDataDir, branchedId, firstId);
    await expectSnapshotParent(device.appDataDir, firstId, null);

    // Branch view renders one node per snapshot.
    await openGame(page);
    await page.getByRole('tab', { name: 'Branch View' }).click();
    await expect(branchNode(page, firstId)).toBeVisible();
    await expect(branchNode(page, secondId)).toBeVisible();
    await expect(branchNode(page, branchedId)).toBeVisible();

    // Continue from here moves only the position, not the save files.
    await branchNodeAction(page, secondId, 'Continue from here');
    await expect(page.getByText('Current position updated').first()).toBeVisible({
      timeout: 15_000,
    });
    await expectLocalHead(device.appDataDir, DEVICE_A_ID, secondId);
    expect(await readFile(device.savePath, 'utf8')).toBe('v1-branched\n');

    // Create snapshot from here parents the new snapshot at the chosen node.
    await branchNodeAction(page, secondId, 'Create snapshot from here');
    const prompt = page.getByRole('dialog', { name: 'Create branch' });
    await expect(prompt).toBeVisible();
    await prompt.getByRole('textbox').fill('child-of-second');
    await prompt.getByRole('button', { name: 'Confirm' }).click();
    await expect
      .poll(async () => (await readBackupsJson(device.appDataDir)).backups.length, {
        timeout: 30_000,
      })
      .toBe(4);
    const fourthId = (await readBackupsJson(device.appDataDir)).backups.find(
      (item) => item.describe === 'child-of-second'
    )!.date;
    await expectSnapshotParent(device.appDataDir, fourthId, secondId);
    await expectLocalHead(device.appDataDir, DEVICE_A_ID, fourthId);

    // Deleting a childless head falls back to its parent.
    await branchNodeAction(page, fourthId, 'Delete');
    await confirmSnapshotDeletion(page, fourthId);
    await expectLocalHead(device.appDataDir, DEVICE_A_ID, secondId);
    expect(localArchiveExists(device.appDataDir, fourthId)).toBe(false);

    // Deleting a head with children moves the position to the newest child and
    // re-parents the children to the deleted node's parent (here: root).
    await branchNodeAction(page, firstId, 'Continue from here');
    await expectLocalHead(device.appDataDir, DEVICE_A_ID, firstId);
    await branchNodeAction(page, firstId, 'Delete');
    await confirmSnapshotDeletion(page, firstId);
    // branchedId sorts after secondId, so it becomes the new position.
    await expectLocalHead(device.appDataDir, DEVICE_A_ID, branchedId);
    await expectSnapshotParent(device.appDataDir, secondId, null);
    await expectSnapshotParent(device.appDataDir, branchedId, null);
    expect(localArchiveExists(device.appDataDir, firstId)).toBe(false);

    // Deleting a root head without children falls back to the newest remaining.
    await branchNodeAction(page, branchedId, 'Delete');
    await confirmSnapshotDeletion(page, branchedId);
    await expectLocalHead(device.appDataDir, DEVICE_A_ID, secondId);
    await expectSnapshotDates(device.appDataDir, [secondId]);

    // The table view still lists the surviving snapshot.
    await page.getByRole('tab', { name: 'Table View' }).click();
    await expect(
      page.locator(`[role="row"][data-snapshot-id=${JSON.stringify(secondId)}]`)
    ).toBeVisible();
  } catch (error) {
    failed = true;
    throw error;
  } finally {
    await session.close(failed);
  }
});
