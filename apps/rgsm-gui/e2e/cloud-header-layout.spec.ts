import { test, expect } from '@playwright/test';
import { seedEmptyCloudWithLocalGame } from './support/cloud-fixture';
import { createLibrary, openGame } from './support/gui';
import { createRunRoot } from './support/rgsm-instance';
import { startDualSession } from './support/session';

test('cloud metadata adds an accessible icon without moving snapshot controls', async ({
  browser,
}) => {
  const runRoot = await createRunRoot('cloud-header');
  const seeded = await seedEmptyCloudWithLocalGame(runRoot);
  const session = await startDualSession(browser, { ...seeded, runRoot, label: 'cloud-header' });
  let failed = false;
  let releaseResponse = () => {};
  try {
    const page = session.pageA;
    await createLibrary(page);
    const responseGate = new Promise<void>((resolve) => {
      releaseResponse = resolve;
    });
    let held = false;
    await page.route('**/api/v1/refresh-cloud-archive-library', async (route) => {
      if (route.request().method() !== 'POST') return route.continue();
      const response = await route.fetch();
      held = true;
      await responseGate;
      await route.fulfill({ response });
    });
    await openGame(page);
    await expect.poll(() => held).toBe(true);
    const create = page.getByRole('button', { name: 'Create new snapshot', exact: true });
    const before = await create.boundingBox();
    expect(before).not.toBeNull();
    releaseResponse();
    const icon = page.getByRole('img', { name: /^(Manual|Cloud Backup|Multi-device Sync)$/ });
    await expect(icon).toBeVisible();
    const after = await create.boundingBox();
    expect(after?.y).toBeCloseTo(before!.y, 0);
    await icon.focus();
    await page.keyboard.press('Tab');
    await page.keyboard.press('Shift+Tab');
    await expect(icon).toBeFocused();
    // Reka puts the tooltip role on a visually hidden label inside the visible bubble.
    const tooltip = page.getByRole('tooltip', { includeHidden: true });
    await expect(tooltip).toHaveText((await icon.getAttribute('aria-label')) ?? '');
    await expect(tooltip.locator('..')).toBeVisible();
  } catch (error) {
    failed = true;
    throw error;
  } finally {
    releaseResponse();
    await session.close(failed);
  }
});
