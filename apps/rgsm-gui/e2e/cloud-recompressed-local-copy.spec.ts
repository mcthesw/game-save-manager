import { test, expect } from '@playwright/test';
import { existsSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { localArchivePath } from './support/archive-path';
import { STORAGE_KEY } from './support/constants';
import { seedEmptyCloudWithLocalGame, writeSave } from './support/cloud-fixture';
import {
  createLibrary,
  createPublishedSnapshot,
  downloadAll,
  openGame,
  refreshCloudView,
  snapshotRow,
} from './support/gui';
import { createRunRoot } from './support/rgsm-instance';
import { startDualSession } from './support/session';

type CatalogRow = Record<string, unknown> & { date: string };

// A local format upgrade that predates the Cloud Library protection leaves
// snapshots whose local archive no longer matches the Cloud copy.
test('repacked local copies are restored from the cloud copy', async ({ browser }) => {
  const runRoot = await createRunRoot('recompressed-local-copy');
  const seeded = await seedEmptyCloudWithLocalGame(runRoot);
  const session = await startDualSession(browser, {
    ...seeded,
    runRoot,
    label: 'recompressed-local-copy',
  });
  let failed = false;
  try {
    await createLibrary(session.pageA);
    const one = await createPublishedSnapshot(session.pageA, session.hostA, 'One');
    await writeSave(seeded.deviceA, 'two\n');
    const two = await createPublishedSnapshot(session.pageA, session.hostA, 'Two');

    const appData = session.hostA.appDataDir;
    const directory = join(appData, 'save_data', STORAGE_KEY);
    const catalogPath = join(directory, 'Backups.json');
    const catalog: { backups: CatalogRow[] } = JSON.parse(readFileSync(catalogPath, 'utf8'));
    const originals = new Map<string, Buffer>();
    const repacked = new Map<string, string>();
    for (const row of catalog.backups) {
      const original = localArchivePath(appData, row.date);
      originals.set(row.date, readFileSync(original));
      rmSync(original);
      const name = `${row.date}_aabbccddeeff.7z`;
      writeFileSync(join(directory, name), 'repacked');
      repacked.set(row.date, join(directory, name));
      Object.assign(row, {
        archive_format: 'seven_z',
        archive_name: name,
        path: join(directory, name),
        size: 8,
        archive_hash: '1111111111111111',
      });
    }
    writeFileSync(catalogPath, JSON.stringify(catalog));

    await openGame(session.pageA);
    await refreshCloudView(session.pageA);
    const download = snapshotRow(session.pageA, one).getByRole('button', {
      name: 'Download to this device',
    });
    await expect(download).toBeVisible({ timeout: 30_000 });
    await download.click();
    await expect(
      snapshotRow(session.pageA, one).getByRole('button', { name: 'Remove from this device' })
    ).toBeVisible({ timeout: 30_000 });
    await expect(session.pageA.getByText('Snapshot transfer failed')).toHaveCount(0);
    expect(readFileSync(localArchivePath(appData, one)).equals(originals.get(one)!)).toBe(true);
    expect(existsSync(repacked.get(one)!)).toBe(false);
    expect(existsSync(repacked.get(two)!)).toBe(true);

    await downloadAll(session.pageA, session.hostA, [two]);
    await expect.poll(() => existsSync(repacked.get(two)!), { timeout: 60_000 }).toBe(false);
    expect(readFileSync(localArchivePath(appData, two)).equals(originals.get(two)!)).toBe(true);
  } catch (error) {
    failed = true;
    throw error;
  } finally {
    await session.close(failed);
  }
});
