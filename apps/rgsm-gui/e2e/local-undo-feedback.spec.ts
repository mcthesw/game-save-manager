import { test, expect } from '@playwright/test';
import { readFile } from 'node:fs/promises';
import { DEVICE_A_ID, GAME_NAME } from './support/constants';
import { seedLocalConfig, writeSaveText } from './support/local-fixture';
import { startLocalSession } from './support/local-session';
import { createRunRoot } from './support/rgsm-instance';
import { createSnapshotForGame } from './support/local-gui';
import { expectLocalHead } from './support/local-assertions';
import { openGame, snapshotRow } from './support/gui';

for (const failure of ['files', 'position', 'connection'] as const) {
  test(`undo reports ${failure} failure without claiming success`, async ({
    browser,
  }, testInfo) => {
    const runRoot = await createRunRoot(`undo-${failure}`);
    const device = await seedLocalConfig(runRoot);
    await writeSaveText(device.savePath, 'first\n');
    const session = await startLocalSession(browser, { runRoot, device, label: 'undo-feedback' });
    let failed = false;
    try {
      const { page, host } = session;
      const first = await createSnapshotForGame(host, GAME_NAME, 'first');
      await writeSaveText(device.savePath, 'second\n');
      await createSnapshotForGame(host, GAME_NAME, 'second');
      await writeSaveText(device.savePath, 'unsaved\n');
      await openGame(page);
      await snapshotRow(page, first).getByRole('button', { name: 'Apply' }).click();
      const undo = page.getByRole('button', { name: 'Undo last apply', exact: true });
      await expect(undo).toBeEnabled();
      await expect(page.locator('.global-loading-overlay')).toBeHidden();
      await expectLocalHead(device.appDataDir, DEVICE_A_ID, first);

      // Only the failing step is injected. Snapshot creation and file restoration
      // otherwise use the real host and isolated save files.
      let positionRequests = 0;
      await page.route('**/api/v1/set-snapshot-head', async (route) => {
        if (route.request().method() !== 'POST') return route.continue();
        positionRequests += 1;
        if (failure === 'connection') return route.abort('connectionfailed');
        await route.fulfill({
          status: 400,
          json: { code: 'invalid_request', message: 'Access is denied' },
        });
      });
      if (failure === 'files') {
        await page.route('**/api/v1/restore-extra-backup', (route) =>
          route.request().method() === 'POST'
            ? route.fulfill({
                status: 400,
                json: { code: 'invalid_request', message: 'Read failed' },
              })
            : route.continue()
        );
      }
      await undo.click();
      await page
        .getByRole('dialog', { name: 'Warning' })
        .getByRole('button', { name: 'Confirm' })
        .click();
      const activity = page.locator('.activity-toast');
      await expect(
        activity.getByText(
          failure === 'files'
            ? 'Failed to undo'
            : 'Game saves restored, but the snapshot list\u0027s "Current Position" marker was not updated',
          { exact: true }
        )
      ).toBeVisible();
      await expect(activity.getByText('Successfully undone the last applied snapshot')).toHaveCount(
        0
      );
      await expect(page.locator('.global-loading-overlay')).toBeHidden();
      expect(await readFile(device.savePath, 'utf8')).toBe(
        failure === 'files' ? 'first\n' : 'unsaved\n'
      );
      await expectLocalHead(device.appDataDir, DEVICE_A_ID, first);
      expect(positionRequests).toBe(failure === 'files' ? 0 : 1);
      if (failure === 'files') {
        await expect(undo).toBeEnabled();
      } else {
        await expect(page.getByRole('button', { name: 'No recent apply to undo' })).toBeDisabled();
      }
      if (failure === 'position') {
        await page.screenshot({
          path: testInfo.outputPath('acceptance-undo-failure.png'),
          animations: 'disabled',
        });
      }
    } catch (error) {
      failed = true;
      throw error;
    } finally {
      await session.close(failed);
    }
  });
}
