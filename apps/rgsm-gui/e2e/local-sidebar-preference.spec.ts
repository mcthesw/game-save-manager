import { expect, test } from '@playwright/test';
import { GAME_NAME } from './support/constants';
import { seedLocalConfig } from './support/local-fixture';
import { getSettings } from './support/local-gui';
import { startLocalSession } from './support/local-session';
import { createRunRoot } from './support/rgsm-instance';

test('sidebar default persists across host restart and favorites can be empty', async ({
  browser,
}) => {
  const runRoot = await createRunRoot('sidebar-preference');
  const device = await seedLocalConfig(runRoot, {
    favorites: [{ node_id: 'favorite-preview', label: GAME_NAME, is_leaf: true, children: null }],
  });
  const options = { runRoot, device, label: 'sidebar-preference' };
  let session = await startLocalSession(browser, options);
  let failed = false;
  try {
    let page = session.page;
    await expect(page.getByRole('tab', { name: 'Favorites', exact: true })).toHaveAttribute(
      'aria-selected',
      'true'
    );
    await page.getByRole('button', { name: 'Settings', exact: true }).click();
    await page.getByRole('button', { name: 'Interface & Appearance', exact: true }).click();
    await page.getByRole('combobox', { name: 'Default game list' }).click();
    await page.getByRole('option', { name: 'All', exact: true }).click();
    await expect
      .poll(async () => (await getSettings(session.host)).appearance)
      .toMatchObject({ default_game_list: 'all' });
    await session.close(true);
    session = await startLocalSession(browser, options);
    page = session.page;
    await expect(page.getByRole('tab', { name: 'All', exact: true })).toHaveAttribute(
      'aria-selected',
      'true'
    );
    await page.getByRole('button', { name: 'Settings', exact: true }).click();
    await page.getByRole('button', { name: 'Interface & Appearance', exact: true }).click();
    await page.getByRole('combobox', { name: 'Default game list' }).click();
    await page.getByRole('option', { name: 'Favorites', exact: true }).click();
    await expect
      .poll(async () => (await getSettings(session.host)).appearance)
      .toMatchObject({ default_game_list: 'favorites' });
    const favoriteRow = page.locator('.fav-row.leaf');
    const rowBefore = await favoriteRow.boundingBox();
    await page.getByRole('button', { name: /^Enable editing:/ }).click();
    expect((await favoriteRow.boundingBox())!.height).toBe(rowBefore!.height);
    const remove = page
      .locator('.fav-row.leaf')
      .getByRole('button', { name: 'Remove from favorites' });
    await expect(remove).toBeVisible();
    await page.screenshot({
      path: test.info().outputPath('acceptance-sidebar-controls.png'),
      animations: 'disabled',
    });
    const saved = page.waitForResponse(
      (response) => response.url().endsWith('/api/v1/set-config') && response.ok()
    );
    await remove.click();
    await saved;
    await expect(page.locator('.fav-row.leaf')).toHaveCount(0);
    await page.reload();
    await expect(page.getByRole('tab', { name: 'Favorites', exact: true })).toHaveAttribute(
      'aria-selected',
      'true'
    );
    await expect(page.locator('.fav-row.leaf')).toHaveCount(0);
    await page.getByRole('tab', { name: 'All', exact: true }).click();
    await expect(page.locator(`button[title="${GAME_NAME}"]`)).toBeVisible();
  } catch (error) {
    failed = true;
    throw error;
  } finally {
    await session.close(failed);
  }
});
