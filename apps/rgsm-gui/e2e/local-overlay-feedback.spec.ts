import { test, expect } from '@playwright/test';
import { readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { GAME_NAME } from './support/constants';
import { seedLocalConfig, writeSaveText } from './support/local-fixture';
import { startLocalSession } from './support/local-session';
import { createRunRoot } from './support/rgsm-instance';
import { createSnapshotForGame, getLocalGame, updateSettings } from './support/local-gui';
import { openGame, snapshotRow } from './support/gui';

test('cloud completion feedback uses the newest history entry', async ({ browser }) => {
  const runRoot = await createRunRoot('cloud-completion-feedback');
  const device = await seedLocalConfig(runRoot);
  const session = await startLocalSession(browser, {
    runRoot,
    device,
    label: 'cloud-completion-feedback',
  });
  let failed = false;
  try {
    const { page } = session;
    // Exercise the status-event view model without requiring a slow cloud transfer.
    await page.evaluate(async () => {
      const path = '/src/composables/useCloudSyncStatus.ts';
      const { useCloudSyncStatus } = await import(path);
      const state = useCloudSyncStatus();
      state.jobs.value = [{ id: 2, description: 'Current upload', status: 'Running' }];
      state.activeJobs.value = 1;
    });
    const toast = page.locator('.activity-toast');
    await expect(toast).toContainText('Current upload');
    await page.evaluate(async () => {
      const path = '/src/composables/useCloudSyncStatus.ts';
      const { useCloudSyncStatus } = await import(path);
      const state = useCloudSyncStatus();
      state.jobs.value = [
        { id: 2, description: 'Current upload', status: 'Completed' },
        { id: 1, description: 'Previous upload', status: 'Failed', error: 'Old failure' },
      ];
      state.activeJobs.value = 0;
    });
    await expect(toast).toContainText('Current upload');
    await expect(toast).not.toContainText('Old failure');
    await expect(toast.locator('.text-success')).toBeVisible();
  } catch (error) {
    failed = true;
    throw error;
  } finally {
    await session.close(failed);
  }
});

test('rename errors remain readable without closing the edit drawer', async ({
  browser,
}, testInfo) => {
  const runRoot = await createRunRoot('rename-feedback');
  const device = await seedLocalConfig(runRoot, {
    games: [GAME_NAME, 'Other game'].map((name, index) => ({
      name,
      units: [{ type: 'File', path: join(runRoot, `save-${index}.txt`) }],
    })),
  });
  const session = await startLocalSession(browser, { runRoot, device, label: 'rename-feedback' });
  let failed = false;
  try {
    const { page, host } = session;
    await openGame(page);
    await page.getByRole('button', { name: 'View managed files' }).click();
    const drawer = page.getByRole('dialog');
    await drawer.getByRole('textbox', { name: 'Game name', exact: true }).fill('OTHER GAME');
    await drawer.getByRole('button', { name: 'save', exact: true }).click();
    const activity = page.locator('.activity-toast');
    await expect(activity.getByText('Game name duplicated', { exact: false })).toBeVisible({
      timeout: 5000,
    });
    await expect(drawer).toBeVisible();
    await expect(page.locator('.activity-drawer')).toBeHidden();
    await expect(drawer.getByRole('button', { name: 'save', exact: true })).toBeFocused();
    await page.screenshot({
      path: testInfo.outputPath('acceptance-rename-feedback.png'),
      animations: 'disabled',
    });
    await drawer.getByRole('button', { name: 'save', exact: true }).click({ trial: true });
    expect((await getLocalGame(host, GAME_NAME)).name).toBe(GAME_NAME);
    await activity.getByRole('button', { name: 'Close', exact: true }).click();
    await expect(drawer).toBeVisible();
    await expect(activity).toHaveCount(0);
    await drawer.getByRole('button', { name: 'Close', exact: true }).click();
    const history = page.locator('.activity-drawer');
    await expect(history).toHaveClass(/is-ghost-tab/);
    await history.locator('.activity-pill').click();
    await expect(history.getByText('Game name duplicated', { exact: false })).toBeVisible();
    await page.getByRole('button', { name: 'View managed files' }).click();
    await expect(history).toBeHidden();
    await expect(activity).toHaveCount(0);
  } catch (error) {
    failed = true;
    throw error;
  } finally {
    await session.close(failed);
  }
});

test('extra restore keeps confirmation, loading and result above the drawer', async ({
  browser,
}, testInfo) => {
  const runRoot = await createRunRoot('restore-feedback');
  const device = await seedLocalConfig(runRoot);
  await writeSaveText(device.savePath, 'snapshot\n');
  const session = await startLocalSession(browser, { runRoot, device, label: 'restore-feedback' });
  let failed = false;
  let releaseResponse = () => {};
  try {
    const { page, host } = session;
    const snapshot = await createSnapshotForGame(host, GAME_NAME, 'First');
    await writeSaveText(device.savePath, 'unsaved progress\n');
    await openGame(page);
    await snapshotRow(page, snapshot).getByRole('button', { name: 'Apply' }).click();
    await expect.poll(() => readFile(device.savePath, 'utf8')).toBe('snapshot\n');
    await updateSettings(host, { confirm_before_apply_snapshot: true });
    await page.reload();
    await page.getByRole('button', { name: 'More actions' }).click();
    await page.getByRole('menuitem', { name: 'Extra backups' }).click();
    const drawer = page.getByRole('dialog', { name: 'Extra backups' });
    const restore = drawer.getByRole('button', { name: 'Apply', exact: true }).first();
    await restore.click();
    const confirmation = page.getByRole('dialog', { name: 'Warning', exact: true });
    await expect(confirmation.getByText('Confirm overwriting existing save?')).toBeVisible();
    await confirmation.getByRole('button', { name: 'Cancel', exact: true }).click();
    await expect(drawer).toBeVisible();
    expect(await readFile(device.savePath, 'utf8')).toBe('snapshot\n');

    // Hold a real backend response to observe the in-progress UI deterministically.
    // Neither the operation nor its response is mocked.
    const responseGate = new Promise<void>((resolve) => {
      releaseResponse = resolve;
    });
    let requests = 0;
    await page.route('**/api/v1/restore-extra-backup', async (route) => {
      if (route.request().method() !== 'POST') return route.continue();
      requests += 1;
      const response = await route.fetch();
      await responseGate;
      await route.fulfill({ response });
    });
    await restore.click();
    await confirmation.getByRole('button', { name: 'Confirm', exact: true }).click();
    const loading = page.locator('.global-loading-overlay');
    await expect(loading).toBeVisible();
    await expect
      .poll(() =>
        loading.evaluate((element) => {
          const rect = element.getBoundingClientRect();
          return element.contains(
            document.elementFromPoint(rect.x + rect.width / 2, rect.y + rect.height / 2)
          );
        })
      )
      .toBe(true);
    await expect(restore).toBeDisabled();
    await page.screenshot({
      path: testInfo.outputPath('acceptance-restore-loading.png'),
      animations: 'disabled',
    });
    await page.keyboard.press('Enter');
    expect(requests).toBe(1);
    await page.keyboard.press('Escape');
    await expect(drawer).toBeVisible();
    await expect(restore).toBeDisabled();
    releaseResponse();
    await expect(loading).toBeHidden();
    await expect(
      page.locator('.activity-toast').getByText('Successfully restored', { exact: true })
    ).toBeVisible();
    await expect(drawer).toBeVisible();
    await expect(restore).toBeEnabled();
    expect(await readFile(device.savePath, 'utf8')).toBe('unsaved progress\n');
    await page.keyboard.press('Escape');
    await expect(drawer).toBeHidden();
  } catch (error) {
    failed = true;
    throw error;
  } finally {
    releaseResponse();
    await session.close(failed);
  }
});
