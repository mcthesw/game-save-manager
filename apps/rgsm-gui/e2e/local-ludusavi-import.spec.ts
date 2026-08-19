import { test, expect } from '@playwright/test';
import { seedLocalConfig } from './support/local-fixture';
import { startLocalSession } from './support/local-session';
import { getLocalGame } from './support/local-gui';
import { createRunRoot } from './support/rgsm-instance';

const IMPORTED_GAME = 'Stardew Valley';

// Onboarding via the bundled Ludusavi manifest: search the full database,
// pick a game, confirm the detected save paths, and the game lands in the
// library with save units populated.
test('ludusavi import: search manifest, customize paths, game joins the library', async ({
  browser,
}) => {
  const runRoot = await createRunRoot('local-import');
  const device = await seedLocalConfig(runRoot, { games: [] });
  const session = await startLocalSession(browser, { runRoot, device, label: 'local-import' });
  const { page, host } = session;
  let failed = false;
  try {
    await page.goto('/');
    await page.getByRole('button', { name: 'Add game' }).first().click();
    await page.getByRole('button', { name: 'Detect local games' }).click();

    const dialog = page.getByRole('dialog', {
      name: 'Import Games (Auto-detect Save Locations)',
    });
    await expect(dialog).toBeVisible();
    // A fresh machine has no locally detected games; search the full manifest.
    await dialog.getByRole('checkbox', { name: 'Show only locally installed games' }).uncheck();
    const search = dialog.getByRole('textbox', { name: 'Search games...' });
    await expect(search).toBeEnabled({ timeout: 120_000 });
    await search.fill(IMPORTED_GAME);
    const row = dialog.getByRole('checkbox', { name: IMPORTED_GAME, exact: true });
    await expect(row).toBeVisible({ timeout: 30_000 });
    await row.check();

    await dialog.getByRole('button', { name: /Import 1 selected game/ }).click();

    const customize = page.getByRole('dialog', { name: `Customize Import: ${IMPORTED_GAME}` });
    await expect(customize).toBeVisible({ timeout: 60_000 });
    // Paths default to unselected; confirm only imports the checked ones.
    await customize.getByRole('button', { name: 'Select all' }).click();
    await customize.getByRole('button', { name: /^confirm$/i }).click();

    // The game joins the library: sidebar entry plus resolved save units.
    await expect(page.locator(`button[title="${IMPORTED_GAME}"]`)).toBeVisible({
      timeout: 30_000,
    });
    const game = await getLocalGame(host, IMPORTED_GAME);
    expect(game.save_paths.length).toBeGreaterThan(0);
  } catch (error) {
    failed = true;
    throw error;
  } finally {
    await session.close(failed);
  }
});
