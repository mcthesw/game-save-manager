import { test, expect } from '@playwright/test';
import { seedLocalConfig } from './support/local-fixture';
import { startLocalSession } from './support/local-session';
import { getLocalGame } from './support/local-gui';
import { createRunRoot, hostPost } from './support/rgsm-instance';

test('batch import with automatic favorites keeps all imported games and binds their identities', async ({
  browser,
}) => {
  const runRoot = await createRunRoot('batch-favorites');
  const device = await seedLocalConfig(runRoot, {
    games: [],
    settings: { add_new_to_favorites: true },
  });
  const session = await startLocalSession(browser, { runRoot, device, label: 'batch-favorites' });
  let failed = false;
  let releaseCheck = () => {};
  try {
    const { page, host } = session;
    await page.getByRole('button', { name: 'Add game' }).first().click();
    await page.getByRole('button', { name: 'Detect local games' }).click();
    const dialog = page.getByRole('dialog', { name: 'Import Games (Auto-detect Save Locations)' });
    await dialog.getByRole('checkbox', { name: 'Show only locally installed games' }).uncheck();
    const search = dialog.getByRole('textbox', { name: 'Search games...' });
    await expect(search).toBeEnabled({ timeout: 120_000 });
    for (const name of ['Stardew Valley', 'Hollow Knight']) {
      await search.fill(name);
      await dialog.getByRole('checkbox', { name, exact: true }).check();
    }
    const checkGate = new Promise<void>((resolve) => {
      releaseCheck = resolve;
    });
    let heldCheck = false;
    await page.route('**/api/v1/check-paths', async (route) => {
      if (heldCheck) return route.continue();
      heldCheck = true;
      const response = await route.fetch();
      await checkGate;
      await route.fulfill({ response });
    });
    await Promise.all([
      page.waitForRequest((request) => request.url().endsWith('/api/v1/check-paths')),
      dialog.getByRole('button', { name: /Import 2 selected game/ }).click(),
    ]);
    const batch = page.getByRole('dialog', { name: 'Batch Import: 2 games' });
    await batch.getByRole('button', { name: 'Select all', exact: true }).click();
    releaseCheck();
    await expect(batch.getByRole('button', { name: 'Verify paths', exact: true })).toBeEnabled();
    for (const name of ['Stardew Valley', 'Hollow Knight']) {
      await expect(batch.getByRole('checkbox', { name, exact: true })).toBeChecked();
    }
    await batch.getByRole('button', { name: /Import 2 selected game/ }).click();
    await expect(batch).not.toBeVisible();
    await expect
      .poll(async () => {
        const result = await hostPost<{
          games: Array<{ name: string; storage_key: string }>;
          favorites: Array<{ game_id?: string }>;
        }>(host, '/api/v1/get-local-config');
        return {
          games: result.data.games.map((game) => game.name).sort(),
          bound: result.data.favorites.filter((favorite) =>
            result.data.games.some((game) => game.storage_key === favorite.game_id)
          ).length,
        };
      })
      .toEqual({ games: ['Hollow Knight', 'Stardew Valley'], bound: 2 });
  } catch (error) {
    failed = true;
    throw error;
  } finally {
    releaseCheck();
    await session.close(failed);
  }
});

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
  let releaseCheck = () => {};
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

    let heldCheck = false;
    const checkGate = new Promise<void>((resolve) => {
      releaseCheck = resolve;
    });
    await page.route('**/api/v1/check-paths', async (route) => {
      if (heldCheck || route.request().postDataJSON().paths.length <= 1) return route.continue();
      heldCheck = true;
      const response = await route.fetch();
      await checkGate;
      await route.fulfill({ response });
    });
    await dialog.getByRole('button', { name: /Import 1 selected game/ }).click();

    const customize = page.getByRole('dialog', { name: `Customize Import: ${IMPORTED_GAME}` });
    await expect(customize).toBeVisible({ timeout: 60_000 });
    await expect.poll(() => heldCheck).toBe(true);
    // A late automatic check must not undo the player's explicit path selection.
    await customize.getByRole('button', { name: 'Select all' }).click();
    releaseCheck();
    await expect(
      customize.getByRole('button', { name: 'Verify paths', exact: true })
    ).toBeEnabled();
    const paths = customize.getByRole('checkbox');
    for (const path of await paths.all()) await expect(path).toBeChecked({ timeout: 5000 });
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
    releaseCheck();
    await session.close(failed);
  }
});
