import { test, expect } from '@playwright/test';
import { readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { DEVICE_A_ID, GAME_NAME } from './support/constants';
import { seedLocalConfig } from './support/local-fixture';
import { startLocalSession } from './support/local-session';
import { createRunRoot } from './support/rgsm-instance';

const SECOND_GAME = 'Glass Harbor';

// Favorites are device-private in the v2 ownership store: they persist into
// the current device's profile file, not the merged config view.
async function readFavoriteLabels(appDataDir: string): Promise<string[]> {
  const profilePath = join(
    appDataDir,
    'GameSaveManager.config.v2',
    'device-profiles',
    `${Buffer.from(DEVICE_A_ID, 'utf8').toString('hex')}.json`
  );
  const raw = await readFile(profilePath, 'utf8');
  const profile = JSON.parse(raw) as {
    private_favorites?: Array<{ label: string; is_leaf: boolean }>;
  };
  return (profile.private_favorites ?? []).filter((node) => node.is_leaf).map((node) => node.label);
}

// Starring a game from the All view must persist into the config file, drive
// the Favorites view after a reload, and unstarring must remove it again.
test('favorites: star persists to config, survives reload, unstar removes', async ({ browser }) => {
  const runRoot = await createRunRoot('local-favorites');
  const device = await seedLocalConfig(runRoot, {
    games: [
      {
        name: GAME_NAME,
        units: [{ type: 'File', path: join(runRoot, 'saves-a', 'progress.txt') }],
      },
      {
        name: SECOND_GAME,
        units: [{ type: 'File', path: join(runRoot, 'saves-b', 'progress.txt') }],
      },
    ],
  });
  const session = await startLocalSession(browser, { runRoot, device, label: 'local-favorites' });
  const { page } = session;
  let failed = false;
  try {
    await page.goto('/');
    // No favorites yet: the sidebar falls back to the All view. The row's
    // accessible name absorbs the nested star button's aria-label, so match
    // on the title attribute instead.
    const row = page.locator(`button[title="${GAME_NAME}"]`);
    await expect(row).toBeVisible();
    await row.getByRole('button', { name: 'Add to Favorites' }).click();
    await expect
      .poll(async () => readFavoriteLabels(device.appDataDir), { timeout: 15_000 })
      .toEqual([GAME_NAME]);

    // After a reload the sidebar defaults to the Favorites view with the game.
    await page.reload();
    await expect(page.getByText(GAME_NAME, { exact: true }).first()).toBeVisible();
    await expect
      .poll(async () => readFavoriteLabels(device.appDataDir), { timeout: 15_000 })
      .toEqual([GAME_NAME]);

    // Unstarring removes the leaf from the config file.
    await page.getByRole('tab', { name: 'All', exact: true }).click();
    await page
      .locator(`button[title="${GAME_NAME}"]`)
      .getByRole('button', { name: 'Remove' })
      .click();
    await expect
      .poll(async () => readFavoriteLabels(device.appDataDir), { timeout: 15_000 })
      .toEqual([]);
  } catch (error) {
    failed = true;
    throw error;
  } finally {
    await session.close(failed);
  }
});
