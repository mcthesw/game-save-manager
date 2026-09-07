import { expect, test } from '@playwright/test';
import { GAME_NAME } from './support/constants';
import { seedLocalConfig, writeSaveText } from './support/local-fixture';
import { getSettings, listSnapshotsFor } from './support/local-gui';
import { startLocalSession } from './support/local-session';
import { openGame, snapshotRow } from './support/gui';
import { createRunRoot } from './support/rgsm-instance';

test('snapshot time defaults to exact, persists a relative preference, and ticks locally', async ({
  browser,
}) => {
  const runRoot = await createRunRoot('snapshot-time');
  const device = await seedLocalConfig(runRoot);
  await writeSaveText(device.savePath, 'time display sample');
  const options = { runRoot, device, label: 'snapshot-time' };
  let session = await startLocalSession(browser, options);
  let failed = false;
  try {
    const { page, host } = session;
    expect((await getSettings(host)).appearance).toMatchObject({
      snapshot_time_format: 'absolute',
    });
    await openGame(page);
    await page.getByRole('button', { name: 'Create new snapshot' }).click();
    await expect.poll(async () => (await listSnapshotsFor(host, GAME_NAME)).length).toBe(1);
    const id = (await listSnapshotsFor(host, GAME_NAME))[0]!.date;
    const time = snapshotRow(page, id).locator('time');
    await expect(time).toHaveText(/^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}$/);
    const exact = await time.textContent();
    await page.clock.install();
    await page.getByRole('button', { name: 'Settings', exact: true }).click();
    await page.getByRole('button', { name: 'Interface & Appearance', exact: true }).click();
    const format = page.getByRole('combobox', { name: 'Snapshot time display' });
    await expect(format).toHaveText('Exact time');
    await format.click();
    await page.getByRole('option', { name: 'Relative time', exact: true }).click();
    await page.clock.runFor(600);
    await expect
      .poll(async () => (await getSettings(host)).appearance)
      .toMatchObject({ snapshot_time_format: 'relative' });
    await page.screenshot({ path: test.info().outputPath('snapshot-time-setting.png') });
    await openGame(page);
    await expect(time).toHaveText('Just now');
    await expect(time).toHaveAttribute('title', exact!.trim());
    await snapshotRow(page, id).getByRole('checkbox').check();
    let dataReads = 0;
    page.on('request', (request) => {
      if (
        /\/(get-game-snapshots-info|refresh-cloud-archive-library)$/.test(
          new URL(request.url()).pathname
        )
      ) {
        dataReads += 1;
      }
    });
    await page.clock.fastForward(65_000);
    await expect(time).toHaveText('1 minute ago');
    await expect(snapshotRow(page, id).getByRole('checkbox')).toBeChecked();
    expect(dataReads).toBe(0);
    await page.screenshot({ path: test.info().outputPath('relative-snapshot-time.png') });

    // Restart the real host as well as the browser to prove device-local persistence.
    await session.close(true);
    session = await startLocalSession(browser, options);
    expect((await getSettings(session.host)).appearance).toMatchObject({
      snapshot_time_format: 'relative',
    });
    await openGame(session.page);
    await expect(snapshotRow(session.page, id).locator('time')).not.toHaveText(/^\d{4}-/);
    await session.page.getByRole('button', { name: 'Settings', exact: true }).click();
    await session.page.getByRole('button', { name: 'Interface & Appearance', exact: true }).click();
    await session.page.getByRole('combobox', { name: 'Snapshot time display' }).click();
    await session.page.getByRole('option', { name: 'Exact time', exact: true }).click();
    await expect
      .poll(async () => (await getSettings(session.host)).appearance)
      .toMatchObject({ snapshot_time_format: 'absolute' });
    await openGame(session.page);
    await expect(snapshotRow(session.page, id).locator('time')).toHaveText(exact!.trim());
  } catch (error) {
    failed = true;
    throw error;
  } finally {
    await session.close(failed);
  }
});
