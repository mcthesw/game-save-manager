import { test, expect } from '@playwright/test';
import { existsSync } from 'node:fs';
import { localArchivePath } from './support/cloud-assertions';
import { readSave, seedEmptyCloudWithLocalGame, writeSave } from './support/cloud-fixture';
import {
  confirmJoinKeepCloud,
  createLibrary,
  createPublishedSnapshot,
  downloadAll,
} from './support/gui';
import { createRunRoot } from './support/rgsm-instance';
import { startDualSession } from './support/session';

test('download all missing cloud copies', async ({ browser }) => {
  const runRoot = await createRunRoot('download-all');
  const seeded = await seedEmptyCloudWithLocalGame(runRoot);
  const session = await startDualSession(browser, { ...seeded, runRoot, label: 'download-all' });
  let failed = false;
  try {
    await createLibrary(session.pageA);
    const one = await createPublishedSnapshot(session.pageA, session.hostA, 'One');
    await writeSave(seeded.deviceA, 'two\n');
    const two = await createPublishedSnapshot(session.pageA, session.hostA, 'Two');
    await confirmJoinKeepCloud(session.pageB);

    const saveB = await readSave(seeded.deviceB);
    await downloadAll(session.pageB, session.hostB, [one, two]);
    expect(existsSync(localArchivePath(seeded.deviceB.appDataDir, one))).toBe(true);
    expect(existsSync(localArchivePath(seeded.deviceB.appDataDir, two))).toBe(true);
    expect(await readSave(seeded.deviceB)).toBe(saveB);
  } catch (error) {
    failed = true;
    throw error;
  } finally {
    await session.close(failed);
  }
});
