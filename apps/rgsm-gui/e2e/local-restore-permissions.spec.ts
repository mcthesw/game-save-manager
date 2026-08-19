import { test, expect } from '@playwright/test';
import { chmod, readFile, rm } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { join } from 'node:path';
import { seedLocalConfig, writeSaveText } from './support/local-fixture';
import { startLocalSession } from './support/local-session';
import { createSnapshotForGame, getLocalGame, type LocalGame } from './support/local-gui';
import { hostPost, createRunRoot, type RgsmHost } from './support/rgsm-instance';

async function restoreRaw(
  host: RgsmHost,
  game: LocalGame,
  date: string
): Promise<{ ok: boolean; raw: string }> {
  const result = await hostPost(host, '/api/v1/restore-snapshot', { game, date });
  return { ok: result.ok, raw: result.raw };
}

async function snapshotOf(host: RgsmHost, gameName: string, describe: string): Promise<string> {
  return createSnapshotForGame(host, gameName, describe);
}

test('restore on read-only targets: posix permission matrix', async ({ browser }) => {
  test.skip(process.platform === 'win32', 'posix-only permission semantics');
  const runRoot = await createRunRoot('local-perm-posix');
  const fileOff = join(runRoot, 'saves-a', 'Perm File Off', 'save.dat');
  const fileOn = join(runRoot, 'saves-a', 'Perm File On', 'save.dat');
  const dirOff = join(runRoot, 'saves-a', 'Perm Dir Off', 'save');
  const dirOn = join(runRoot, 'saves-a', 'Perm Dir On', 'save');
  const device = await seedLocalConfig(runRoot, {
    games: [
      { name: 'Perm File Off', units: [{ type: 'File', path: fileOff }] },
      {
        name: 'Perm File On',
        units: [{ type: 'File', path: fileOn, deleteBeforeApply: true }],
      },
      { name: 'Perm Dir Off', units: [{ type: 'Folder', path: dirOff }] },
      {
        name: 'Perm Dir On',
        units: [{ type: 'Folder', path: dirOn, deleteBeforeApply: true }],
      },
    ],
  });
  await writeSaveText(fileOff, 'v1\n');
  await writeSaveText(fileOn, 'v1\n');
  await writeSaveText(join(dirOff, 'keep.txt'), 'off-v1\n');
  await writeSaveText(join(dirOn, 'keep.txt'), 'on-v1\n');

  const session = await startLocalSession(browser, { runRoot, device, label: 'local-perm-posix' });
  const { host } = session;
  let failed = false;
  try {
    const idOff = await snapshotOf(host, 'Perm File Off', 'base');
    const idOn = await snapshotOf(host, 'Perm File On', 'base');
    const idDirOff = await snapshotOf(host, 'Perm Dir Off', 'base');
    const idDirOn = await snapshotOf(host, 'Perm Dir On', 'base');

    // Mutate live saves, then lock them down.
    await writeSaveText(fileOff, 'v2\n');
    await writeSaveText(fileOn, 'v2\n');
    await chmod(fileOff, 0o444);
    await chmod(fileOn, 0o444);
    await writeSaveText(join(dirOff, 'keep.txt'), 'off-v2\n');
    await chmod(join(dirOff, 'keep.txt'), 0o444);
    await chmod(dirOff, 0o555);
    await writeSaveText(join(dirOn, 'dirty.txt'), 'dirty\n');
    await chmod(dirOn, 0o555);

    // Read-only file, plain overwrite: must fail without touching current data.
    const gameOff = await getLocalGame(host, 'Perm File Off');
    const failOff = await restoreRaw(host, gameOff, idOff);
    expect(failOff.ok, failOff.raw).toBe(false);
    expect(await readFile(fileOff, 'utf8')).toBe('v2\n');

    // Read-only file, delete-before-overwrite: unlink + recreate succeeds.
    const gameOn = await getLocalGame(host, 'Perm File On');
    const okOn = await restoreRaw(host, gameOn, idOn);
    expect(okOn.ok, okOn.raw).toBe(true);
    expect(await readFile(fileOn, 'utf8')).toBe('v1\n');

    // Read-only directory tree, plain overwrite: must fail, keep.txt intact.
    const gameDirOff = await getLocalGame(host, 'Perm Dir Off');
    const failDirOff = await restoreRaw(host, gameDirOff, idDirOff);
    expect(failDirOff.ok, failDirOff.raw).toBe(false);
    expect(await readFile(join(dirOff, 'keep.txt'), 'utf8')).toBe('off-v2\n');

    // Read-only directory tree, delete-before-overwrite: the restore makes the
    // tree removable, clears it (dirty file included), then extracts.
    const gameDirOn = await getLocalGame(host, 'Perm Dir On');
    const okDirOn = await restoreRaw(host, gameDirOn, idDirOn);
    expect(okDirOn.ok, okDirOn.raw).toBe(true);
    expect(await readFile(join(dirOn, 'keep.txt'), 'utf8')).toBe('on-v1\n');
    expect(existsSync(join(dirOn, 'dirty.txt'))).toBe(false);
  } catch (error) {
    failed = true;
    throw error;
  } finally {
    // Restore writability so runRoot cleanup cannot trip over read-only bits.
    await chmod(fileOff, 0o644).catch(() => {});
    await chmod(fileOn, 0o644).catch(() => {});
    await chmod(join(dirOff, 'keep.txt'), 0o644).catch(() => {});
    await chmod(dirOff, 0o755).catch(() => {});
    await chmod(dirOn, 0o755).catch(() => {});
    await session.close(failed);
  }
});

test('restore over hidden and read-only files on Windows', async ({ browser }) => {
  test.skip(process.platform !== 'win32', 'windows-only file attributes');
  const runRoot = await createRunRoot('local-perm-win');
  const hiddenOff = join(runRoot, 'saves-a', 'Win Hidden Off', 'save.dat');
  const hiddenOn = join(runRoot, 'saves-a', 'Win Hidden On', 'save.dat');
  const roOff = join(runRoot, 'saves-a', 'Win RO Off', 'save.dat');
  const roOn = join(runRoot, 'saves-a', 'Win RO On', 'save.dat');
  const device = await seedLocalConfig(runRoot, {
    games: [
      { name: 'Win Hidden Off', units: [{ type: 'File', path: hiddenOff }] },
      {
        name: 'Win Hidden On',
        units: [{ type: 'File', path: hiddenOn, deleteBeforeApply: true }],
      },
      { name: 'Win RO Off', units: [{ type: 'File', path: roOff }] },
      {
        name: 'Win RO On',
        units: [{ type: 'File', path: roOn, deleteBeforeApply: true }],
      },
    ],
  });
  await writeSaveText(hiddenOff, 'v1\n');
  await writeSaveText(hiddenOn, 'v1\n');
  await writeSaveText(roOff, 'v1\n');
  await writeSaveText(roOn, 'v1\n');

  const session = await startLocalSession(browser, { runRoot, device, label: 'local-perm-win' });
  const { host } = session;
  const winPath = (path: string) => path.replaceAll('/', '\\');
  const attrib = (args: string[]) =>
    expect(spawnSync('cmd', ['/c', 'attrib', ...args]).status, `attrib ${args}`).toBe(0);
  let failed = false;
  try {
    const idHiddenOff = await snapshotOf(host, 'Win Hidden Off', 'base');
    const idHiddenOn = await snapshotOf(host, 'Win Hidden On', 'base');
    const idRoOff = await snapshotOf(host, 'Win RO Off', 'base');
    const idRoOn = await snapshotOf(host, 'Win RO On', 'base');
    await writeSaveText(hiddenOff, 'v2\n');
    await writeSaveText(hiddenOn, 'v2\n');
    await writeSaveText(roOff, 'v2\n');
    await writeSaveText(roOn, 'v2\n');
    attrib(['+h', winPath(hiddenOff)]);
    attrib(['+h', winPath(hiddenOn)]);
    attrib(['+r', winPath(roOff)]);
    attrib(['+r', winPath(roOn)]);

    // Hidden files accept a plain overwrite; the restored copy keeps the
    // archived hidden attribute.
    const gameHiddenOff = await getLocalGame(host, 'Win Hidden Off');
    const okHiddenOff = await restoreRaw(host, gameHiddenOff, idHiddenOff);
    expect(okHiddenOff.ok, okHiddenOff.raw).toBe(true);
    expect(await readFile(hiddenOff, 'utf8')).toBe('v1\n');

    const gameHiddenOn = await getLocalGame(host, 'Win Hidden On');
    const okHiddenOn = await restoreRaw(host, gameHiddenOn, idHiddenOn);
    expect(okHiddenOn.ok, okHiddenOn.raw).toBe(true);
    expect(await readFile(hiddenOn, 'utf8')).toBe('v1\n');

    // Read-only files reject plain overwrite without touching current data;
    // delete-before-overwrite removes and recreates them.
    const gameRoOff = await getLocalGame(host, 'Win RO Off');
    const failRoOff = await restoreRaw(host, gameRoOff, idRoOff);
    expect(failRoOff.ok, failRoOff.raw).toBe(false);
    expect(await readFile(roOff, 'utf8')).toBe('v2\n');

    const gameRoOn = await getLocalGame(host, 'Win RO On');
    const okRoOn = await restoreRaw(host, gameRoOn, idRoOn);
    expect(okRoOn.ok, okRoOn.raw).toBe(true);
    expect(await readFile(roOn, 'utf8')).toBe('v1\n');
  } catch (error) {
    failed = true;
    throw error;
  } finally {
    for (const path of [hiddenOff, hiddenOn, roOff, roOn]) {
      spawnSync('cmd', ['/c', 'attrib', '-h', '-r', winPath(path)]);
      await rm(path, { force: true }).catch(() => {});
    }
    await session.close(failed);
  }
});
