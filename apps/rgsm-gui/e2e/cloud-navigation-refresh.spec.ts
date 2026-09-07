import { expect, test, type Page } from '@playwright/test';
import { seedEmptyCloudWithLocalGame } from './support/cloud-fixture';
import { GAME_NAME } from './support/constants';
import { createLibrary, createPublishedSnapshot, snapshotRow } from './support/gui';
import { addGameViaApi } from './support/local-gui';
import { startLocalSession } from './support/local-session';
import { createRunRoot } from './support/rgsm-instance';

const SECOND_GAME = 'Glass Harbor';
const cloudPath = '/api/v1/refresh-cloud-archive-library';
const localPath = '/api/v1/get-game-snapshots-info';

async function refresh(page: Page) {
  await page.evaluate(async () => {
    const path = '/src/composables/useCloudLibrary.ts';
    await (await import(path)).refreshCloudLibrary(true);
  });
}

async function switchGame(page: Page, name: string) {
  const response = page.waitForResponse(
    (response) => new URL(response.url()).pathname === localPath
  );
  await page.locator(`.all-list button[title="${name}"]`).click();
  await response;
  await expect(page.getByRole('button', { name: 'Create new snapshot' })).toBeVisible();
}

test('game navigation reuses cloud metadata and background refresh preserves the loaded list', async ({
  browser,
}) => {
  const runRoot = await createRunRoot('cloud-navigation');
  const seeded = await seedEmptyCloudWithLocalGame(runRoot);
  const session = await startLocalSession(browser, {
    runRoot,
    device: seeded.deviceA,
    label: 'cloud-navigation',
  });
  const { page, host } = session;
  let release = () => {};
  let delayedRefresh: Promise<void> | undefined;
  let failed = false;
  try {
    await page.clock.install();
    await createLibrary(page);
    const snapshot = await createPublishedSnapshot(page, host, 'Keep selected');
    await addGameViaApi(host, SECOND_GAME, [{ type: 'File', path: seeded.deviceA.savePath }]);
    await refresh(page);
    await page.getByRole('tab', { name: 'All', exact: true }).click();
    await expect(page.locator(`.all-list button[title="${SECOND_GAME}"]`)).toBeVisible();
    await expect(snapshotRow(page, snapshot)).toBeVisible();
    let cloudReads = 0;
    let localReads = 0;
    page.on('request', (request) => {
      const path = new URL(request.url()).pathname;
      if (path === cloudPath) cloudReads += 1;
      if (path === localPath) localReads += 1;
    });
    for (const name of [SECOND_GAME, GAME_NAME, SECOND_GAME, GAME_NAME]) {
      await switchGame(page, name);
    }
    await expect(snapshotRow(page, snapshot)).toBeVisible();
    expect(cloudReads).toBe(0);
    expect(localReads).toBe(4);

    await snapshotRow(page, snapshot).getByRole('checkbox').check();
    const readsBeforeRefresh = localReads;
    await refresh(page);
    expect(cloudReads).toBe(1);
    expect(localReads).toBe(readsBeforeRefresh);
    await expect(snapshotRow(page, snapshot).getByRole('checkbox')).toBeChecked();

    // A different game's definition must not reload this game's local catalog.
    await addGameViaApi(host, 'New cloud game', [{ type: 'File', path: seeded.deviceA.savePath }]);
    await refresh(page);
    expect(cloudReads).toBe(2);
    expect(localReads).toBe(readsBeforeRefresh);
    await expect(snapshotRow(page, snapshot).getByRole('checkbox')).toBeChecked();
    await page.evaluate(async () => {
      window.dispatchEvent(new Event('focus'));
      document.dispatchEvent(new Event('visibilitychange'));
      const path = '/src/composables/useCloudLibrary.ts';
      await (await import(path)).refreshCloudLibraryIfStale();
    });
    expect(cloudReads).toBe(2);

    // The periodic check is scheduled from the last completed read, not navigation.
    await page.clock.fastForward(5 * 60_000 + 1000);
    await page.evaluate(async () => {
      const path = '/src/composables/useCloudLibrary.ts';
      await (await import(path)).refreshCloudLibraryIfStale();
    });
    expect(cloudReads).toBe(3);
    expect(localReads).toBe(readsBeforeRefresh);

    // Passing the configured refresh interval allows one shared foreground read.
    const expiredTime = await page.evaluate(() => Date.now() + 6 * 60_000);
    await page.clock.setSystemTime(new Date(expiredTime));
    await page.evaluate(async () => {
      window.dispatchEvent(new Event('focus'));
      document.dispatchEvent(new Event('visibilitychange'));
      const path = '/src/composables/useCloudLibrary.ts';
      await (await import(path)).refreshCloudLibraryIfStale();
    });
    expect(cloudReads).toBe(4);
    expect(localReads).toBe(readsBeforeRefresh);

    // A slow cloud response must not block local navigation or snapshot actions.
    let captured = false;
    const gate = new Promise<void>((resolve) => {
      release = resolve;
    });
    await page.route(`**${cloudPath}`, async (route) => {
      const response = await route.fetch();
      captured = true;
      await gate;
      await route.fulfill({ response });
    });
    delayedRefresh = refresh(page);
    await expect.poll(() => captured).toBe(true);
    await switchGame(page, SECOND_GAME);
    await switchGame(page, GAME_NAME);
    await expect(snapshotRow(page, snapshot)).toBeVisible();
    await expect(
      snapshotRow(page, snapshot).getByRole('button', { name: 'Apply', exact: true })
    ).toBeEnabled();
    release();
    await delayedRefresh;
    await page.unroute(`**${cloudPath}`);
  } catch (error) {
    failed = true;
    throw error;
  } finally {
    release();
    await delayedRefresh?.catch(() => {});
    await session.close(failed);
  }
});
