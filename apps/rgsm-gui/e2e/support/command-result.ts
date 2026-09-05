import type { Page } from '@playwright/test';

/** Observe this operation, not a notification left by an earlier request. */
export async function waitForCommand(
  page: Page,
  path: string,
  snapshotId: string,
  action: () => Promise<unknown>
): Promise<void> {
  const [response] = await Promise.all([
    page.waitForResponse(
      (response) =>
        new URL(response.url()).pathname === path &&
        response.request().method() === 'POST' &&
        response.request().postDataJSON()?.date === snapshotId,
      { timeout: 15_000 }
    ),
    action(),
  ]);
  const body = await response.text();
  if (!response.ok()) {
    throw new Error(`${path} failed (${response.status()}): ${body}`);
  }
}
