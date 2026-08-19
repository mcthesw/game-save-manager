import { test, expect } from '@playwright/test';
import { readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { DEVICE_A_ID, GAME_NAME } from './support/constants';
import { seedLocalConfig, writeSaveText } from './support/local-fixture';
import { startLocalSession } from './support/local-session';
import { expectLocalHead } from './support/local-assertions';
import { listSnapshotsFor, type LocalSnapshotInfo } from './support/local-gui';
import { createRunRoot } from './support/rgsm-instance';

const SECOND_GAME = 'Glass Harbor';

async function confirmYes(page: import('@playwright/test').Page): Promise<void> {
  const prompt = page.getByRole('dialog');
  await prompt.getByRole('textbox').fill('yes');
  await prompt.getByRole('button', { name: 'Confirm' }).click();
}

// Settings-page batch operations must hit every game: Backup all creates one
// snapshot per game, Apply all restores each game's latest snapshot.
test('backup all and apply all operate on every game', async ({ browser }) => {
  const runRoot = await createRunRoot('local-batch-all');
  const saveA = join(runRoot, 'saves-a', 'progress.txt');
  const saveB = join(runRoot, 'saves-b', 'progress.txt');
  const device = await seedLocalConfig(runRoot, {
    games: [
      { name: GAME_NAME, units: [{ type: 'File', path: saveA }] },
      { name: SECOND_GAME, units: [{ type: 'File', path: saveB }] },
    ],
  });
  await writeSaveText(saveA, 'a-v1\n');
  await writeSaveText(saveB, 'b-v1\n');
  const session = await startLocalSession(browser, { runRoot, device, label: 'local-batch-all' });
  const { page, host } = session;
  let failed = false;
  try {
    await page.goto('/');
    await page.getByRole('button', { name: 'Settings', exact: true }).click();
    await page.getByRole('button', { name: 'Backup settings' }).click();
    await page.getByRole('button', { name: 'Backup all saves' }).click();
    await confirmYes(page);

    let snapshotsA: LocalSnapshotInfo[] = [];
    let snapshotsB: LocalSnapshotInfo[] = [];
    await expect
      .poll(
        async () => {
          snapshotsA = await listSnapshotsFor(host, GAME_NAME);
          snapshotsB = await listSnapshotsFor(host, SECOND_GAME);
          return snapshotsA.length + snapshotsB.length;
        },
        { timeout: 30_000 }
      )
      .toBe(2);
    expect(snapshotsA[0]!.describe).toBe('Backup all');
    expect(snapshotsB[0]!.describe).toBe('Backup all');
    // Backup must not touch live files.
    expect(await readFile(saveA, 'utf8')).toBe('a-v1\n');
    expect(await readFile(saveB, 'utf8')).toBe('b-v1\n');

    await writeSaveText(saveA, 'a-v2-live\n');
    await writeSaveText(saveB, 'b-v2-live\n');
    await page.getByRole('button', { name: 'Apply all backups' }).click();
    await confirmYes(page);
    await expect.poll(async () => readFile(saveA, 'utf8'), { timeout: 30_000 }).toBe('a-v1\n');
    await expect.poll(async () => readFile(saveB, 'utf8'), { timeout: 30_000 }).toBe('b-v1\n');
    await expectLocalHead(device.appDataDir, DEVICE_A_ID, snapshotsA[0]!.date, GAME_NAME);
    await expectLocalHead(device.appDataDir, DEVICE_A_ID, snapshotsB[0]!.date, SECOND_GAME);
  } catch (error) {
    failed = true;
    throw error;
  } finally {
    await session.close(failed);
  }
});
