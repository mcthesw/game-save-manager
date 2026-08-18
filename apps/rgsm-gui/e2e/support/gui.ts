import { expect, type Locator, type Page } from '@playwright/test';
import type { RgsmHost } from './rgsm-instance';
import { fsSession, hostPost } from './rgsm-instance';
import { GAME_NAME, V2_ACTIVE_ERROR } from './constants';

export async function openApp(page: Page): Promise<void> {
  await page.goto('/');
  const deviceSetup = page.getByRole('dialog', { name: /Device Setup|设备设置/ });
  if (await deviceSetup.isVisible().catch(() => false)) {
    await page.getByRole('button', { name: /Confirm|确定/ }).click();
    await expect(deviceSetup).toBeHidden();
  }
  await expect(cloudSyncNav(page)).toBeVisible();
}

export function cloudSyncNav(page: Page) {
  return page
    .getByRole('button', { name: 'Cloud sync', exact: true })
    .or(page.getByRole('button', { name: '同步', exact: true }));
}

export async function openSyncSettings(page: Page): Promise<void> {
  await cloudSyncNav(page).click();
  await expect(page.getByRole('heading', { name: /Sync settings|同步设置/ })).toBeVisible();
}
export async function expectLibraryKind(
  page: Page,
  kind: 'cutover' | 'join' | 'active' | 'resume'
): Promise<void> {
  if (kind === 'cutover') {
    await expect(page.getByText('Upgrade required')).toBeVisible();
    await expect(page.getByRole('button', { name: 'Upgrade to the new library' })).toBeVisible();
    return;
  }
  if (kind === 'resume') {
    await expect(page.getByText('Last upgrade did not finish')).toBeVisible();
    await expect(page.getByRole('button', { name: 'Resume upgrade' })).toBeVisible();
    return;
  }
  if (kind === 'join') {
    await expect(page.getByText('Join required')).toBeVisible();
    await expect(page.getByRole('button', { name: 'Join library' })).toBeVisible();
    return;
  }
  await expect(page.getByRole('button', { name: 'Sync mode' })).toBeVisible();
}

export async function confirmCutover(page: Page, resumable = false): Promise<void> {
  await page
    .getByRole('button', { name: resumable ? 'Resume upgrade' : 'Upgrade to the new library' })
    .click();
  const dialog = page.getByRole('dialog', {
    name: resumable ? 'Resume Cloud Library upgrade' : 'Upgrade to the new Cloud Library',
  });
  await expect(dialog).toBeVisible();
  await dialog
    .getByRole('button', { name: resumable ? 'Resume upgrade' : 'Start upgrade' })
    .click();
}

export async function expectCutoverError(page: Page): Promise<void> {
  await expect(page.getByText(/Could not upgrade the legacy Cloud Library/).first()).toBeAttached({
    timeout: 30_000,
  });
}
export async function expectCutoverSuccess(page: Page): Promise<void> {
  await expect(page.getByText('Cloud Library upgraded').first()).toBeAttached({ timeout: 60_000 });
}

export async function confirmJoinKeepCloud(page: Page): Promise<void> {
  await page.getByRole('button', { name: 'Join library' }).first().click();
  const dialog = page.getByRole('dialog');
  await expect(dialog).toBeVisible();
  await dialog.getByRole('button', { name: 'Join library' }).click();
  await expect(page.getByText('This device joined the Cloud Library').first()).toBeAttached({
    timeout: 30_000,
  });
}

export async function openGame(page: Page, gameName = GAME_NAME): Promise<void> {
  await page.goto(`/Management/${encodeURIComponent(gameName)}`);
  await expect(page.getByRole('button', { name: 'Create new snapshot' })).toBeVisible();
}

export async function createSnapshotFromGui(page: Page, describe: string): Promise<void> {
  await page.getByLabel('New backup description').fill(describe);
  await page.getByRole('button', { name: 'Create new snapshot' }).click();
  await expect(page.getByText(/Backup successful/).first()).toBeAttached({ timeout: 30_000 });
}

export function snapshotRow(page: Page, snapshotId: string): Locator {
  return page.getByRole('row').filter({ hasText: snapshotId });
}

export async function uploadSnapshot(page: Page, snapshotId: string): Promise<void> {
  await snapshotRow(page, snapshotId).getByRole('button', { name: 'Upload to cloud' }).click();
  await expect(page.getByText('Snapshot uploaded').first()).toBeAttached({ timeout: 30_000 });
}

export async function downloadSnapshot(page: Page, snapshotId: string): Promise<void> {
  await snapshotRow(page, snapshotId)
    .getByRole('button', { name: 'Download to this device' })
    .click();
  await expect(page.getByText('Snapshot downloaded').first()).toBeAttached({ timeout: 30_000 });
}

export async function deleteCurrentHead(page: Page, snapshotId: string): Promise<void> {
  await snapshotRow(page, snapshotId).getByRole('button', { name: 'Delete' }).click();
  await page.getByRole('button', { name: 'Delete permanently' }).click();
  const fallback = page.getByRole('dialog', { name: 'Permanently delete Snapshot' });
  if (await fallback.isVisible().catch(() => false)) {
    await fallback.getByRole('button', { name: 'Delete permanently' }).click();
  }
  await expect(page.getByText('Successfully deleted').first()).toBeAttached({ timeout: 30_000 });
}

export async function openProgressReview(page: Page): Promise<void> {
  await openSyncSettings(page);
  await page.getByRole('button', { name: 'Progress diverged, compare' }).click();
  await expect(page.getByRole('dialog').getByText('Devices have different progress')).toBeVisible();
}

export async function acceptRemoteProgress(page: Page, snapshotId: string): Promise<void> {
  const dialog = page.getByRole('dialog');
  await dialog
    .locator('article')
    .filter({ hasText: snapshotId })
    .getByRole('button', {
      name: 'Use this progress',
    })
    .click();
  await page.getByRole('button', { name: 'Use this progress' }).last().click();
  await expect(page.getByText(/The selected progress was applied/).first()).toBeAttached({
    timeout: 30_000,
  });
}

export async function changeGameMode(page: Page, modeLabel: string): Promise<void> {
  await openSyncSettings(page);
  await page.getByRole('button', { name: 'Sync mode' }).click();
  await page.getByRole('menuitem', { name: modeLabel }).click();
  const enable = page.getByRole('dialog').getByRole('button', { name: 'Enable' });
  if (await enable.isVisible().catch(() => false)) {
    await enable.click();
  }
  await expect(page.getByText('Sync mode updated').first()).toBeAttached({ timeout: 15_000 });
}

export async function toggleCloudEnabled(page: Page): Promise<void> {
  await openSyncSettings(page);
  await page.getByRole('switch', { name: 'Cloud sync' }).click();
  await expect(page.getByText('Sync mode updated').first()).toBeAttached({ timeout: 15_000 });
}

export async function expectLegacyUploadBlocked(host: RgsmHost, cloudRoot: string): Promise<void> {
  const result = await hostPost<{ message?: string }>(host, '/api/v1/cloud-upload-all', {
    session: fsSession(cloudRoot),
  });
  expect(result.ok).toBe(false);
  expect(`${result.data.message ?? ''} ${result.raw}`).toContain(V2_ACTIVE_ERROR);
}

export async function inspectLibrary(host: RgsmHost) {
  const result = await hostPost<{ kind: string; resumable?: boolean; game_count?: number }>(
    host,
    '/api/v1/inspect-cloud-library'
  );
  expect(result.ok, result.raw).toBe(true);
  return result;
}

export async function getGeneration(host: RgsmHost) {
  const result = await hostPost<string>(host, '/api/v1/get-cloud-namespace-generation');
  expect(result.ok, result.raw).toBe(true);
  return result.data;
}

export async function getCurrentDevice(host: RgsmHost) {
  const result = await hostPost<{ id: string }>(host, '/api/v1/get-current-device-info');
  expect(result.ok, result.raw).toBe(true);
  return result.data;
}

export async function getLocalConfig(host: RgsmHost) {
  const result = await hostPost<{
    games: Array<{ name: string; storage_key: string }>;
    settings: { locale: string; cloud_settings: { backend: { type: string }; root_path: string } };
    devices: Record<string, { id: string }>;
  }>(host, '/api/v1/get-local-config');
  expect(result.ok, result.raw).toBe(true);
  return result.data;
}

export async function createSnapshotViaApi(host: RgsmHost, describe: string) {
  const config = await getLocalConfig(host);
  const game = config.games.find((item) => item.name === GAME_NAME);
  expect(game).toBeTruthy();
  const result = await hostPost(host, '/api/v1/create-snapshot', {
    game,
    describe,
  });
  expect(result.ok, result.raw).toBe(true);
}

export async function uploadArchiveViaApi(host: RgsmHost, snapshotId: string) {
  const result = await hostPost(host, '/api/v1/upload-cloud-archive', {
    gameId: GAME_NAME,
    snapshotId,
  });
  expect(result.ok, result.raw).toBe(true);
}

export async function downloadArchiveViaApi(host: RgsmHost, snapshotId: string) {
  const result = await hostPost(host, '/api/v1/download-cloud-archive', {
    gameId: GAME_NAME,
    snapshotId,
  });
  expect(result.ok, result.raw).toBe(true);
}

export async function reviewProgress(host: RgsmHost) {
  const result = await hostPost<{
    requires_choice: boolean;
    candidates: Array<{ snapshot_id: string }>;
    local?: { snapshot_id: string };
  }>(host, '/api/v1/review-v2-game-progress', { gameId: GAME_NAME });
  expect(result.ok, result.raw).toBe(true);
  return result.data;
}

export async function acceptRemoteViaApi(
  host: RgsmHost,
  selectedSnapshotId: string,
  expectedLocalSnapshotId: string | null,
  manifestRevision: number
) {
  const result = await hostPost(host, '/api/v1/accept-v2-remote-progress', {
    gameId: GAME_NAME,
    manifestRevision,
    expectedLocalSnapshotId,
    selectedSnapshotId,
  });
  expect(result.ok, result.raw).toBe(true);
}

export async function getArchiveLibrary(host: RgsmHost) {
  const result = await hostPost<{
    revision: number;
    games: Array<{
      game_id: string;
      sync_mode: string;
      cloud_sync_enabled: boolean;
      snapshots: Array<{ snapshot_id: string }>;
    }>;
  }>(host, '/api/v1/get-cloud-archive-library');
  expect(result.ok, result.raw).toBe(true);
  return result.data;
}
