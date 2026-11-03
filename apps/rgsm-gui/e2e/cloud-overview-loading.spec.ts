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
  let failed = false;
  try {
    const cutover = await hostPost(session.host, '/api/v1/cutover-cloud-library', {
      confirmed: true,
    });
    expect(cutover.ok, cutover.raw).toBe(true);
    await session.page.reload();
    let cloudReads = 0;
    session.page.on('request', (request) => {
      const path = new URL(request.url()).pathname;
      if (
        path === '/api/v1/inspect-cloud-library' ||
        path === '/api/v1/refresh-cloud-archive-library'
      ) {
        cloudReads += 1;
      }
    });
    await openSyncSettings(session.page);
    await expect(
      session.page.getByRole('button', { name: 'Check library', exact: true })
    ).toHaveCount(0);
    await expect(
      session.page.getByText('Cloud connection not configured', { exact: true })
    ).toHaveCount(0);
    await session.page.waitForTimeout(1000);
    // Opening the page shows what is already known and never contacts the provider.
    expect(cloudReads).toBe(0);
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
    await session.close(failed);
  }
});
