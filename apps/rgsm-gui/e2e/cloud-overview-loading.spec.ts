import { expect, test } from '@playwright/test';
import { seedLegacyV1Scene } from './support/cloud-fixture';
import { openSyncSettings } from './support/gui';
import { startLocalSession } from './support/local-session';
import { createRunRoot, hostPost } from './support/rgsm-instance';

test('cloud overview does not show setup actions while inspection is pending', async ({
  browser,
}) => {
  const runRoot = await createRunRoot('overview-loading');
  const seeded = await seedLegacyV1Scene(runRoot);
  const session = await startLocalSession(browser, {
    runRoot,
    device: seeded.deviceA,
    label: 'overview-loading',
  });
  let release!: () => void;
  const gate = new Promise<void>((resolve) => {
    release = resolve;
  });
  let received!: () => void;
  const responseReady = new Promise<void>((resolve) => {
    received = resolve;
  });
  let failed = false;
  try {
    const cutover = await hostPost(session.host, '/api/v1/cutover-cloud-library', {
      confirmed: true,
    });
    expect(cutover.ok, cutover.raw).toBe(true);
    await session.page.reload();
    await session.page.route('**/api/v1/inspect-cloud-library', async (route) => {
      const response = await route.fetch();
      received();
      await gate;
      await route.fulfill({ response });
    });
    await openSyncSettings(session.page);
    await responseReady;
    await expect(
      session.page.getByRole('button', { name: 'Check library', exact: true })
    ).toHaveCount(0);
    await expect(
      session.page.getByText('Cloud connection not configured', { exact: true })
    ).toHaveCount(0);
    release();
    const tutorial = session.page.getByRole('button', { name: 'Tutorial', exact: true });
    await tutorial.hover();
    await expect(
      session.page.getByText('Tutorial', { exact: true }).filter({ visible: true })
    ).toBeVisible();
    await session.page.screenshot({
      path: test.info().outputPath('acceptance-cloud-overview.png'),
      animations: 'disabled',
    });
  } catch (error) {
    failed = true;
    throw error;
  } finally {
    release();
    await session.close(failed);
  }
});
