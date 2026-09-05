import { expect, test } from '@playwright/test';
import { DEVICE_A_ID, DEVICE_B_ID, GAME_NAME, STORAGE_KEY } from './support/constants';
import { writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { seedEmptyCloudWithLocalGame } from './support/cloud-fixture';
import {
  createLibrary,
  createPublishedSnapshot,
  connectLibrary,
  openGame,
  downloadSnapshot,
  snapshotRow,
} from './support/gui';
import { listSnapshotsFor } from './support/local-gui';
import { readBackupsJson, snapshotMeta } from './support/local-assertions';
import { createRunRoot } from './support/rgsm-instance';
import { startDualSession } from './support/session';

test('a headless copied catalog selects its first parent by displayed creation time', async ({
  browser,
}) => {
  const runRoot = await createRunRoot('snapshot-choice');
  const seeded = await seedEmptyCloudWithLocalGame(runRoot);
  const session = await startDualSession(browser, { ...seeded, runRoot, label: 'snapshot-choice' });
  let failed = false;
  try {
    await createLibrary(session.pageA);
    const parent = await createPublishedSnapshot(session.pageA, session.hostA, 'Original');
    await connectLibrary(session.pageB);
    await openGame(session.pageB);
    await downloadSnapshot(session.pageB, parent);
    expect(
      (await readBackupsJson(seeded.deviceB.appDataDir)).device_heads?.[DEVICE_B_ID]
    ).toBeUndefined();
    // A catalog copied from another device retains that device's position.
    // Ordinary download only imports archives; it does not set either position.
    const catalog = await readBackupsJson(seeded.deviceB.appDataDir);
    catalog.device_heads = { [DEVICE_A_ID]: parent };
    await writeFile(
      join(seeded.deviceB.archiveRoot, STORAGE_KEY, 'Backups.json'),
      JSON.stringify(catalog)
    );
    await openGame(session.pageB);
    const displayedTime = await snapshotRow(session.pageB, parent)
      .getByRole('cell')
      .nth(1)
      .locator('span')
      .first()
      .innerText();
    await session.pageB.getByPlaceholder('New backup description').fill('Continued here');
    await session.pageB.getByRole('button', { name: 'Create new snapshot' }).click();
    const base = session.pageB.getByRole('dialog', { name: 'Choose first snapshot base' });
    await base.getByRole('textbox').fill('2', { timeout: 10_000 });
    await base.getByRole('button', { name: 'Confirm' }).click();
    const choice = session.pageB.getByRole('dialog', { name: 'Choose parent snapshot' });
    await expect(choice).toContainText(displayedTime);
    await choice.getByRole('textbox').fill(displayedTime);
    await choice.getByRole('button', { name: 'Confirm' }).click();
    await expect
      .poll(
        async () =>
          (await listSnapshotsFor(session.hostB, GAME_NAME)).find(
            (snapshot) => snapshot.describe === 'Continued here'
          )?.parent,
        { timeout: 10_000 }
      )
      .toBe(parent);
    expect((await snapshotMeta(seeded.deviceB.appDataDir, parent)).device_id).not.toBe(DEVICE_B_ID);
  } catch (error) {
    failed = true;
    throw error;
  } finally {
    await session.close(failed);
  }
});
