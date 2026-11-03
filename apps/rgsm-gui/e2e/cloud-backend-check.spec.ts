import { expect, test } from '@playwright/test';
import { seedLegacyV1Scene } from './support/cloud-fixture';
import { openSyncSettings } from './support/gui';
import { startLocalSession } from './support/local-session';
import { createRunRoot } from './support/rgsm-instance';

test('a failed connection check explains the failed item when it is clicked', async ({
  browser,
}) => {
  const runRoot = await createRunRoot('backend-check');
  const seeded = await seedLegacyV1Scene(runRoot);
  const session = await startLocalSession(browser, {
    runRoot,
    device: seeded.deviceA,
    label: 'backend-check',
  });
  let failed = false;
  try {
    await session.page.route('**/api/v1/check-cloud-backend', (route) =>
      route.fulfill({
        json: {
          outcome: 'unavailable',
          items: [
            { step: 'prepare_backend', status: 'passed', message: null },
            {
              step: 'list_files',
              status: 'failed',
              message: 'WebDAV returned 429 Too Many Requests',
            },
          ],
        },
      })
    );
    await openSyncSettings(session.page);
    await session.page.getByRole('button', { name: 'Cloud settings' }).click();
    await session.page.getByRole('button', { name: 'Test availability' }).click();
    await expect(session.page.getByRole('button', { name: 'View error' })).toHaveCount(0);
    await session.page.getByRole('button', { name: 'Read the Cloud Library' }).click();
    await expect(session.page.getByText('The provider is limiting requests')).toBeVisible();
    await expect(session.page.getByText('WebDAV returned 429 Too Many Requests')).toBeVisible();
  } catch (error) {
    failed = true;
    throw error;
  } finally {
    await session.close(failed);
  }
});
