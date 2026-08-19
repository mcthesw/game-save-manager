import { test, expect } from '@playwright/test';
import { readFile, stat, utimes } from 'node:fs/promises';
import { join } from 'node:path';
import { GAME_NAME } from './support/constants';
import { seedLocalConfig, writeSaveText } from './support/local-fixture';
import { startLocalSession } from './support/local-session';
import {
  archiveFileName,
  localArchiveExists,
  localSnapshotsDir,
  snapshotMeta,
} from './support/local-assertions';
import { applySnapshotViaApi, createSnapshotForGame, updateSettings } from './support/local-gui';
import { createRunRoot } from './support/rgsm-instance';

// Compression presets must all round-trip: archives restore content intact and
// keep file metadata, regardless of the configured level.
test('compression presets all capture and restore correctly', async ({ browser }) => {
  const runRoot = await createRunRoot('local-compression');
  const device = await seedLocalConfig(runRoot);
  const session = await startLocalSession(browser, { runRoot, device, label: 'local-compression' });
  const { host } = session;
  let failed = false;
  try {
    for (const preset of ['Store', 'Fast', 'Standard', 'Best'] as const) {
      await updateSettings(host, { compression_preset: preset });
      const content = `content-${preset.toLowerCase()}\n`;
      await writeSaveText(device.savePath, content);
      const past = new Date('2020-06-15T08:09:10.000Z');
      await utimes(device.savePath, past, past);

      const date = await createSnapshotForGame(host, GAME_NAME, `preset-${preset}`);
      const meta = await snapshotMeta(device.appDataDir, date);
      expect(meta.archive_format).toBe('seven_z');
      expect(localArchiveExists(device.appDataDir, date)).toBe(true);
      const archiveStat = await stat(
        join(localSnapshotsDir(device.appDataDir), archiveFileName(date))
      );
      expect(archiveStat.size).toBeGreaterThan(0);

      await writeSaveText(device.savePath, `clobbered-${preset.toLowerCase()}\n`);
      await applySnapshotViaApi(host, GAME_NAME, date);
      expect(await readFile(device.savePath, 'utf8')).toBe(content);
      expect((await stat(device.savePath)).mtimeMs).toBe(past.getTime());
    }
  } catch (error) {
    failed = true;
    throw error;
  } finally {
    await session.close(failed);
  }
});
