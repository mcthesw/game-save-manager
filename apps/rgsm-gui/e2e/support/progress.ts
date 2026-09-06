import { expect, type Page } from '@playwright/test';

export async function pendingProgress(page: Page) {
  await page.bringToFront();
  await page.evaluate(async () => {
    const modulePath = '/src/composables/useCloudLibrary.ts';
    await (await import(modulePath)).refreshCloudLibrary();
  });
  const dialog = page.getByRole('dialog', { name: 'Progress available from other devices' });
  await expect(dialog).toBeVisible();
  return dialog;
}

/** Use the actual player action when the scenario continues with manual transfers. */
export async function deferPendingProgress(page: Page) {
  const dialog = await pendingProgress(page);
  await dialog.getByRole('button', { name: 'Later', exact: true }).click();
  await expect(dialog).toBeHidden();
}
