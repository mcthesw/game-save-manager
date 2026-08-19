import { test, expect } from '@playwright/test';
import { readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { GAME_NAME } from './support/constants';
import { seedLocalConfig, writeSaveText } from './support/local-fixture';
import { startLocalSession } from './support/local-session';
import { expectTreeMtimes, listTree, mtimeMs, setMtimeMs } from './support/local-assertions';
import { applySnapshotViaApi, createSnapshotForGame } from './support/local-gui';
import { createRunRoot } from './support/rgsm-instance';

// Fixed timestamps (whole seconds) well in the past, so a restore that forgets
// to reapply mtimes is visible as "now" instead.
const ROOT_FILE_MTIME = 1_704_251_044_000;
const NESTED_DIR_MTIME = 1_704_164_645_000;
const NESTED_FILE_MTIME = 1_704_164_646_000;
const SAVE_DIR_MTIME = 1_703_500_000_000;
const FILE_UNIT_MTIME = 1_703_000_000_000;

test('apply preserves file and nested directory mtimes', async ({ browser }) => {
  const runRoot = await createRunRoot('local-mtime');
  const saveDir = join(runRoot, 'saves-a', GAME_NAME, 'folder-save');
  const nestedDir = join(saveDir, 'nested');
  const fileUnit = join(runRoot, 'saves-a', GAME_NAME, 'progress.txt');
  const device = await seedLocalConfig(runRoot, {
    games: [
      {
        name: GAME_NAME,
        units: [
          { type: 'Folder', path: saveDir },
          { type: 'File', path: fileUnit },
        ],
      },
    ],
  });
  await writeSaveText(join(saveDir, 'root.dat'), 'root-v1\n');
  await writeSaveText(join(nestedDir, 'save.dat'), 'nested-v1\n');
  await writeSaveText(fileUnit, 'file-v1\n');
  // Set mtimes bottom-up: touching a child bumps the parent directory.
  await setMtimeMs(join(saveDir, 'root.dat'), ROOT_FILE_MTIME);
  await setMtimeMs(join(nestedDir, 'save.dat'), NESTED_FILE_MTIME);
  await setMtimeMs(nestedDir, NESTED_DIR_MTIME);
  await setMtimeMs(fileUnit, FILE_UNIT_MTIME);
  const expected = await listTree(saveDir);
  await setMtimeMs(saveDir, SAVE_DIR_MTIME);

  const session = await startLocalSession(browser, { runRoot, device, label: 'local-mtime' });
  const { host } = session;
  let failed = false;
  try {
    const snapshotId = await createSnapshotForGame(host, GAME_NAME, 'timed');

    // New playthrough: content and mtimes all move on.
    await writeSaveText(join(saveDir, 'root.dat'), 'root-v2-longer\n');
    await writeSaveText(join(nestedDir, 'save.dat'), 'nested-v2\n');
    await writeSaveText(fileUnit, 'file-v2\n');
    expect(Math.abs((await mtimeMs(join(saveDir, 'root.dat'))) - ROOT_FILE_MTIME)).toBeGreaterThan(
      10_000
    );

    await applySnapshotViaApi(host, GAME_NAME, snapshotId);

    expect(await readFile(join(saveDir, 'root.dat'), 'utf8')).toBe('root-v1\n');
    expect(await readFile(join(nestedDir, 'save.dat'), 'utf8')).toBe('nested-v1\n');
    expect(await readFile(fileUnit, 'utf8')).toBe('file-v1\n');
    // Files and every directory level get their archived mtimes back.
    await expectTreeMtimes(saveDir, expected);
    expect(Math.abs((await mtimeMs(saveDir)) - SAVE_DIR_MTIME)).toBeLessThanOrEqual(2000);
    expect(Math.abs((await mtimeMs(fileUnit)) - FILE_UNIT_MTIME)).toBeLessThanOrEqual(2000);
  } catch (error) {
    failed = true;
    throw error;
  } finally {
    await session.close(failed);
  }
});
