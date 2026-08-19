import { test, expect } from '@playwright/test';
import { readFile } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import { join } from 'node:path';
import { GAME_NAME } from './support/constants';
import { seedLocalConfig, writeSaveText } from './support/local-fixture';
import { startLocalSession } from './support/local-session';
import {
  applySnapshotViaApi,
  createSnapshotForGame,
  getLocalGame,
  listSnapshotsFor,
} from './support/local-gui';
import { openGame } from './support/gui';
import { createRunRoot } from './support/rgsm-instance';

const SECOND_GAME = 'Second Star';

test('save units: disable skips backup and restore; delete-before-overwrite clears extras', async ({
  browser,
}) => {
  const runRoot = await createRunRoot('local-units');
  const folderUnit = join(runRoot, 'saves-a', GAME_NAME, 'data');
  const plainDir = join(runRoot, 'saves-a', SECOND_GAME, 'plain');
  const wipedDir = join(runRoot, 'saves-a', SECOND_GAME, 'wiped');
  const device = await seedLocalConfig(runRoot, {
    games: [
      {
        name: GAME_NAME,
        units: [
          { type: 'File', path: join(runRoot, 'saves-a', GAME_NAME, 'progress.txt') },
          { type: 'Folder', path: folderUnit },
        ],
      },
      {
        name: SECOND_GAME,
        units: [
          { type: 'Folder', path: plainDir, deleteBeforeApply: false },
          { type: 'Folder', path: wipedDir, deleteBeforeApply: true },
        ],
      },
    ],
  });
  const progressFile = join(runRoot, 'saves-a', GAME_NAME, 'progress.txt');
  const slotFile = join(folderUnit, 'slot1.dat');
  await writeSaveText(progressFile, 'progress-1\n');
  await writeSaveText(slotFile, 'slot-1\n');
  await writeSaveText(join(plainDir, 'keep.txt'), 'plain-1\n');
  await writeSaveText(join(wipedDir, 'keep.txt'), 'wiped-1\n');

  const session = await startLocalSession(browser, { runRoot, device, label: 'local-units' });
  const { page, host } = session;
  let failed = false;
  try {
    const firstId = await createSnapshotForGame(host, GAME_NAME, 'both units');

    // Disable the folder unit through the save-location drawer.
    await openGame(page);
    await page.getByRole('button', { name: 'View managed files' }).click();
    const drawer = page.getByRole('dialog');
    await expect(drawer.getByRole('switch', { name: 'Participate in backup' })).toHaveCount(2);
    await drawer.getByRole('switch', { name: 'Participate in backup' }).nth(1).click();
    await drawer.getByRole('button', { name: 'save', exact: true }).click();
    await expect
      .poll(async () => (await getLocalGame(host, GAME_NAME)).save_paths[1]?.enabled, {
        timeout: 15_000,
      })
      .toBe(false);

    // A snapshot taken now must not contain the folder unit.
    await writeSaveText(progressFile, 'progress-2\n');
    await writeSaveText(slotFile, 'slot-2\n');
    const secondId = await createSnapshotForGame(host, GAME_NAME, 'file only');

    await writeSaveText(progressFile, 'progress-3\n');
    await writeSaveText(slotFile, 'slot-3\n');
    await applySnapshotViaApi(host, GAME_NAME, secondId);
    expect(await readFile(progressFile, 'utf8')).toBe('progress-2\n');
    // Disabled unit stays untouched on restore.
    expect(await readFile(slotFile, 'utf8')).toBe('slot-3\n');

    // Re-enable through the drawer; the old snapshot restores the folder again.
    await page.getByRole('button', { name: 'View managed files' }).click();
    await drawer.getByRole('button', { name: 'Re-enable' }).click();
    await drawer.getByRole('button', { name: 'save', exact: true }).click();
    await expect
      .poll(async () => (await getLocalGame(host, GAME_NAME)).save_paths[1]?.enabled, {
        timeout: 15_000,
      })
      .toBe(true);

    await applySnapshotViaApi(host, GAME_NAME, secondId);
    // The file-only snapshot has no folder data even with the unit enabled.
    expect(await readFile(slotFile, 'utf8')).toBe('slot-3\n');
    await applySnapshotViaApi(host, GAME_NAME, firstId);
    expect(await readFile(progressFile, 'utf8')).toBe('progress-1\n');
    expect(await readFile(slotFile, 'utf8')).toBe('slot-1\n');

    // delete_before_apply: off keeps untracked files, on wipes the target first.
    await createSnapshotForGame(host, SECOND_GAME, 'two folders');
    await writeSaveText(join(plainDir, 'keep.txt'), 'plain-2\n');
    await writeSaveText(join(plainDir, 'dirty.txt'), 'dirty\n');
    await writeSaveText(join(wipedDir, 'keep.txt'), 'wiped-2\n');
    await writeSaveText(join(wipedDir, 'dirty.txt'), 'dirty\n');

    const snapshots = await listSnapshotsFor(host, SECOND_GAME);
    const baseSnapshot = snapshots.find((item) => item.describe === 'two folders')!;
    await applySnapshotViaApi(host, SECOND_GAME, baseSnapshot.date);

    expect(await readFile(join(plainDir, 'keep.txt'), 'utf8')).toBe('plain-1\n');
    expect(existsSync(join(plainDir, 'dirty.txt'))).toBe(true);
    expect(await readFile(join(wipedDir, 'keep.txt'), 'utf8')).toBe('wiped-1\n');
    expect(existsSync(join(wipedDir, 'dirty.txt'))).toBe(false);
  } catch (error) {
    failed = true;
    throw error;
  } finally {
    await session.close(failed);
  }
});
