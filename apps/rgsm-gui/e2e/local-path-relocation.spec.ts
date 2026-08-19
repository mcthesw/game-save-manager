import { test, expect, type Page } from '@playwright/test';
import { readFile } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import { join } from 'node:path';
import { DEVICE_A_ID, GAME_NAME } from './support/constants';
import { seedLocalConfig, writeSaveText } from './support/local-fixture';
import { startLocalSession } from './support/local-session';
import { expectLocalHead } from './support/local-assertions';
import { createSnapshotForGame, getLocalGame } from './support/local-gui';
import { expectActivity, openGame, snapshotRow } from './support/gui';
import { createRunRoot, type RgsmHost } from './support/rgsm-instance';

/** Types a new path into the unit row's contenteditable path editor and saves. */
async function editUnitPathViaDrawer(page: Page, host: RgsmHost, nextPath: string): Promise<void> {
  await page.getByRole('button', { name: 'View managed files' }).click();
  const drawer = page.getByRole('dialog');
  const editor = drawer.locator('.pvi-editor').last();
  await editor.click();
  await page.keyboard.press('ControlOrMeta+A');
  await page.keyboard.type(nextPath.replaceAll('\\', '/'));
  await drawer.getByRole('button', { name: 'save', exact: true }).click();
  const expected = nextPath.replaceAll('\\', '/');
  await expect
    .poll(
      async () => {
        const game = await getLocalGame(host, GAME_NAME);
        const unit = game.save_paths[0];
        return unit?.source.type === 'concrete' ? unit.source.paths?.[DEVICE_A_ID] : undefined;
      },
      { timeout: 15_000 }
    )
    .toBe(expected);
}

test('edit save path: apply follows the current path, invalid path fails then fix works', async ({
  browser,
}) => {
  const runRoot = await createRunRoot('local-relocate');
  const pathA = join(runRoot, 'saves-a', GAME_NAME, 'progress.txt');
  const pathB = join(runRoot, 'relocated', GAME_NAME, 'progress.txt');
  const blockerFile = join(runRoot, 'blocker');
  const pathBlocked = join(blockerFile, 'child', 'progress.txt');
  const pathC = join(runRoot, 'third', 'progress.txt');
  const device = await seedLocalConfig(runRoot, {
    games: [{ name: GAME_NAME, units: [{ type: 'File', path: pathA }] }],
  });
  await writeSaveText(pathA, 'relocate-me\n');
  const session = await startLocalSession(browser, { runRoot, device, label: 'local-relocate' });
  const { page, host } = session;
  let failed = false;
  try {
    const snapshotId = await createSnapshotForGame(host, GAME_NAME, 'original');
    await writeSaveText(pathA, 'edited-after-snapshot\n');

    // Player moved the save elsewhere; after editing the path, Apply must
    // restore to the new location instead of the archived absolute path A.
    await openGame(page);
    await editUnitPathViaDrawer(page, host, pathB);
    await snapshotRow(page, snapshotId).getByRole('button', { name: 'Apply' }).click();
    await expect
      .poll(async () => readFile(pathB, 'utf8').catch(() => null), { timeout: 30_000 })
      .toBe('relocate-me\n');
    expect(await readFile(pathA, 'utf8')).toBe('edited-after-snapshot\n');

    // A path that cannot be created (parent is a file) surfaces a restore error.
    await writeSaveText(blockerFile, 'not a directory\n');
    await editUnitPathViaDrawer(page, host, pathBlocked);
    await snapshotRow(page, snapshotId).getByRole('button', { name: 'Apply' }).click();
    await expectActivity(page, 'Recovery failed');
    await expectLocalHead(device.appDataDir, DEVICE_A_ID, snapshotId);
    expect(existsSync(pathBlocked)).toBe(false);

    // Fixing the path makes the same snapshot apply cleanly.
    await editUnitPathViaDrawer(page, host, pathC);
    await snapshotRow(page, snapshotId).getByRole('button', { name: 'Apply' }).click();
    await expect
      .poll(async () => readFile(pathC, 'utf8').catch(() => null), { timeout: 30_000 })
      .toBe('relocate-me\n');
  } catch (error) {
    failed = true;
    throw error;
  } finally {
    await session.close(failed);
  }
});
