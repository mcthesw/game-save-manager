import { expect, type Locator, type Page } from '@playwright/test';
import { existsSync } from 'node:fs';
import { localArchivePath } from './cloud-assertions';
import type { RgsmHost } from './rgsm-instance';
import { fsSession, hostPost } from './rgsm-instance';
import { GAME_NAME, V2_ACTIVE_ERROR } from './constants';
import { reportTiming } from './timing';

export async function expectActivity(page: Page, pattern: string | RegExp): Promise<void> {
  const drawer = page.locator('.activity-drawer');
  const text = drawer.getByText(pattern).first();
  // The drawer may be collapsed; a loading overlay can also swallow the first
  // click on the pill. Retry until the entry shows or the deadline passes.
  const deadline = Date.now() + 30_000;
  while (Date.now() < deadline) {
    if (await text.isVisible().catch(() => false)) return;
    // The global loading overlay also carries role="status"; target the drawer.
    // Click a fixed safe point in the pill. The drawer can expand between the
    // visibility check and this click; clicking the drawer's moving centre can
    // then land on "Dismiss all" or an activity's dismiss button.
    await drawer
      .locator('.activity-pill')
      .click({ position: { x: 10, y: 10 } })
      .catch(() => {});
    await page.waitForTimeout(500);
  }
  await expect(text).toBeVisible();
}

export async function openApp(page: Page): Promise<void> {
  const startedAt = performance.now();
  await page.goto('/');
  const deviceSetup = page.getByRole('dialog', { name: /Device Setup|设备设置/ });
  if (await deviceSetup.isVisible().catch(() => false)) {
    await page.getByRole('button', { name: /Confirm|确定/ }).click();
    await expect(deviceSetup).toBeHidden();
  }
  await expect(cloudSyncNav(page)).toBeVisible();
  reportTiming('app page ready', startedAt);
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
  kind: 'cutover' | 'join' | 'active' | 'resume' | 'empty'
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
  if (kind === 'empty') {
    await expect(
      page.getByText('This location is empty and can create a new Cloud Library.')
    ).toBeVisible();
    await expect(page.getByRole('button', { name: 'Create library' })).toBeVisible();
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
  await page.goto('/SyncSettings');
  await expectLibraryKind(page, 'join');
  await page.getByRole('button', { name: 'Join library' }).first().click();
  const dialog = page.getByRole('dialog');
  await expect(dialog).toBeVisible();
  await dialog.getByRole('button', { name: 'Join library' }).click();
  await expectActivity(page, 'This device joined the Cloud Library');
}

export async function openGame(page: Page, gameName = GAME_NAME): Promise<void> {
  await page.goto(`/Management/${encodeURIComponent(gameName)}`);
  await expect(page.getByRole('button', { name: 'Create new snapshot' })).toBeVisible();
}

export async function createSnapshotFromGui(
  page: Page,
  host: RgsmHost,
  describe: string
): Promise<string> {
  await page.getByPlaceholder('New backup description').fill(describe);
  await page.getByRole('button', { name: 'Create new snapshot' }).click();
  const firstBase = page.getByRole('dialog', { name: 'Choose first snapshot base' });
  if (await firstBase.isVisible().catch(() => false)) {
    await firstBase.getByRole('textbox').fill('1');
    await firstBase.getByRole('button', { name: 'Confirm' }).click();
  }
  await expect
    .poll(
      async () => {
        const snapshots = await listSnapshots(host);
        return snapshots.find((item) => item.describe === describe)?.date ?? null;
      },
      { timeout: 30_000 }
    )
    .not.toBeNull();
  return latestSnapshotId(host, describe);
}

export function snapshotRow(page: Page, snapshotId: string): Locator {
  return page.getByRole('row').filter({ hasText: snapshotId });
}

export async function uploadSnapshot(page: Page, snapshotId: string): Promise<void> {
  const row = snapshotRow(page, snapshotId);
  const upload = row.getByRole('button', { name: 'Upload to cloud' });
  const uploaded = row.getByRole('button', { name: 'Remove cloud copy' });
  // Wait for the row to settle into either state instead of probing once.
  await expect(upload.or(uploaded)).toBeVisible({ timeout: 30_000 });
  if (await upload.isVisible().catch(() => false)) {
    await upload.click();
  }
  await expect(uploaded).toBeVisible({ timeout: 30_000 });
}

export async function downloadSnapshot(page: Page, snapshotId: string): Promise<void> {
  const row = snapshotRow(page, snapshotId);
  const download = row.getByRole('button', { name: 'Download to this device' });
  const remove = row.getByRole('button', { name: 'Remove from this device' });
  // The row buttons render after the row itself; wait for either state first.
  await download.or(remove).first().waitFor({ state: 'visible', timeout: 30_000 });
  if (await download.isVisible().catch(() => false)) {
    await download.click();
  }
  await expect(remove).toBeVisible({ timeout: 30_000 });
}

export async function deleteCurrentHead(page: Page, snapshotId: string): Promise<void> {
  await snapshotRow(page, snapshotId).getByRole('button', { name: 'Delete' }).click();
  await page.getByRole('button', { name: 'Delete permanently' }).click();
  const fallback = page.getByRole('dialog', { name: 'Permanently delete Snapshot' });
  if (await fallback.isVisible().catch(() => false)) {
    await fallback.getByRole('button', { name: 'Delete permanently' }).click();
  }
  await expectActivity(page, 'Successfully deleted');
}

export async function openProgressReview(page: Page): Promise<void> {
  await openSyncSettings(page);
  const compare = page.getByRole('button', { name: 'Progress diverged, compare' });
  await expect(compare).toBeVisible({ timeout: 30_000 });
  await compare.click();
  await expect(page.getByRole('dialog').getByText('Devices have different progress')).toBeVisible();
}

export async function acceptRemoteProgress(page: Page, snapshotId: string): Promise<void> {
  const review = page.getByRole('dialog', { name: /Compare progress/ });
  const candidate = review
    .locator('article')
    .filter({ hasText: snapshotId })
    .or(
      review
        .locator('article')
        .filter({ has: page.getByRole('button', { name: 'Use this progress' }) })
    )
    .first();
  await candidate.getByRole('button', { name: 'Use this progress' }).click();
  const confirm = page.getByRole('dialog', { name: "Apply another device's progress?" });
  await expect(confirm).toBeVisible();
  await confirm.getByRole('button', { name: 'Use this progress' }).click();
  await expect(page.getByText(/The selected progress was applied/).first()).toBeAttached({
    timeout: 30_000,
  });
}

export async function changeGameMode(page: Page, modeLabel: string): Promise<void> {
  await openSyncSettings(page);
  const modeButton = page.getByRole('button', { name: 'Sync mode' });
  await expect(modeButton).toBeVisible({ timeout: 30_000 });
  const current = (await modeButton.textContent()) ?? '';
  if (current.includes(modeLabel)) {
    return;
  }
  await modeButton.click();
  await page.getByRole('menuitem', { name: modeLabel }).click();
  const enable = page.getByRole('dialog').getByRole('button', { name: 'Enable' });
  if (await enable.isVisible().catch(() => false)) {
    await enable.click();
  }
  await expect(modeButton).toContainText(modeLabel, { timeout: 30_000 });
}

export async function toggleCloudEnabled(
  page: Page,
  host: RgsmHost,
  enabled: boolean
): Promise<void> {
  await openSyncSettings(page);
  const current = await getArchiveLibrary(host);
  const game = current.games.find((item) => item.game_id === GAME_NAME);
  if (game?.cloud_sync_enabled === enabled) {
    return;
  }
  await page.getByRole('switch', { name: 'Cloud sync' }).click();
  await expect
    .poll(
      async () =>
        (await getArchiveLibrary(host)).games.find((item) => item.game_id === GAME_NAME)
          ?.cloud_sync_enabled,
      { timeout: 15_000 }
    )
    .toBe(enabled);
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

// Snapshot IDs are second-precision (`YYYY-MM-DD_HH-mm-ss`); two creations in
// the same second collide (self-parent, overwritten archive). Space them out.
let lastSnapshotCreateMs = 0;

export async function waitForFreshSnapshotSecond(): Promise<void> {
  const elapsed = Date.now() - lastSnapshotCreateMs;
  if (elapsed < 1100) {
    const { promise, resolve } = Promise.withResolvers<void>();
    setTimeout(resolve, 1100 - elapsed);
    await promise;
  }
  lastSnapshotCreateMs = Date.now();
}

export async function createSnapshotViaApi(host: RgsmHost, describe: string) {
  await waitForFreshSnapshotSecond();
  const config = await getLocalConfig(host);
  const game = config.games.find((item) => item.name === GAME_NAME);
  expect(game).toBeTruthy();
  const result = await hostPost(host, '/api/v1/create-snapshot', {
    game,
    describe,
  });
  expect(result.ok, result.raw).toBe(true);
}

export async function listSnapshots(
  host: RgsmHost
): Promise<Array<{ date: string; describe?: string }>> {
  const config = await getLocalConfig(host);
  const game = config.games.find((item) => item.name === GAME_NAME);
  const result = await hostPost<{ backups: Array<{ date: string; describe?: string }> }>(
    host,
    '/api/v1/get-game-snapshots-info',
    { game }
  );
  expect(result.ok, result.raw).toBe(true);
  return result.data.backups;
}

export async function latestSnapshotId(host: RgsmHost, describe?: string): Promise<string> {
  const snapshots = await listSnapshots(host);
  const match = describe ? snapshots.find((item) => item.describe === describe) : snapshots.at(-1);
  expect(match, `missing snapshot ${describe ?? 'latest'}`).toBeTruthy();
  return match!.date;
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

export async function createLibrary(page: Page): Promise<void> {
  await openSyncSettings(page);
  await expectLibraryKind(page, 'empty');
  await page.getByRole('button', { name: 'Create library' }).click();
  await expectActivity(page, 'Cloud Library created');
  await page.reload();
  await openApp(page);
}
export async function applySnapshot(page: Page, snapshotId: string): Promise<void> {
  await snapshotRow(page, snapshotId).getByRole('button', { name: 'Apply' }).click();
  await expectActivity(page, /Successfully restored/);
}

export async function enableMode(
  page: Page,
  host: RgsmHost,
  modeLabel: 'Cloud Backup' | 'Multi-device Sync',
  catchUp: 'Keep in cloud' | 'Download to this device'
): Promise<void> {
  await openSyncSettings(page);
  const modeButton = page.getByRole('button', { name: 'Sync mode' });
  await expect(modeButton).toBeVisible({ timeout: 30_000 });
  const current = (await modeButton.textContent()) ?? '';
  if (current.includes(modeLabel) && catchUp === 'Keep in cloud') {
    return;
  }
  if (current.includes(modeLabel)) {
    await changeGameMode(page, 'Manual');
  }
  await page.getByRole('button', { name: 'Sync mode' }).click();
  await page.getByRole('menuitem', { name: modeLabel }).click();
  const dialog = page.getByRole('dialog');
  await expect(dialog).toBeVisible();
  await dialog.getByText(catchUp, { exact: true }).click();
  await dialog.getByRole('button', { name: 'Enable' }).click();
  await expect(page.getByRole('button', { name: 'Sync mode' })).toContainText(modeLabel, {
    timeout: 30_000,
  });
  if (catchUp === 'Download to this device') {
    await expect
      .poll(
        async () => {
          const library = await getArchiveLibrary(host);
          const game = library.games.find((item) => item.game_id === GAME_NAME);
          return (
            game?.snapshots.every((snapshot) =>
              existsSync(localArchivePath(host.appDataDir, snapshot.snapshot_id))
            ) ?? false
          );
        },
        { timeout: 60_000 }
      )
      .toBe(true);
  }
}

export async function keepLocalProgress(page: Page): Promise<void> {
  await openProgressReview(page);
  const review = page.getByRole('dialog', { name: /Compare progress/ });
  await review.getByRole('button', { name: 'Keep this device' }).click();
  const confirm = page.getByRole('dialog', { name: "Keep this device's progress?" });
  await expect(confirm).toBeVisible();
  await confirm.getByRole('button', { name: 'Keep this device' }).click();
  await expect(confirm).toBeHidden({ timeout: 30_000 });
  await expect(review).toBeHidden({ timeout: 30_000 });
}

export async function evictLocalCopy(page: Page, snapshotId: string): Promise<void> {
  await snapshotRow(page, snapshotId)
    .getByRole('button', { name: 'Remove from this device' })
    .click();
  await page.getByRole('button', { name: 'Remove local copy' }).click();
  await expect(
    snapshotRow(page, snapshotId).getByRole('button', { name: 'Download to this device' })
  ).toBeVisible({ timeout: 15_000 });
}

export async function evictCloudCopy(page: Page, snapshotId: string): Promise<void> {
  await snapshotRow(page, snapshotId).getByRole('button', { name: 'Remove cloud copy' }).click();
  await page.getByRole('dialog').getByRole('button', { name: 'Remove cloud copy' }).click();
  await expect(
    snapshotRow(page, snapshotId).getByRole('button', { name: 'Upload to cloud' })
  ).toBeVisible({ timeout: 15_000 });
}

export async function downloadAll(
  page: Page,
  host: RgsmHost,
  expectedIds: string[]
): Promise<void> {
  await openSyncSettings(page);
  await page.getByRole('button', { name: /Download all to this device|Resume download/ }).click();
  await page
    .getByRole('dialog')
    .getByRole('button', { name: 'Download all to this device' })
    .click();
  await expect
    .poll(() => expectedIds.every((id) => existsSync(localArchivePath(host.appDataDir, id))), {
      timeout: 60_000,
    })
    .toBe(true);
}

export async function permanentlyDeleteGame(page: Page): Promise<void> {
  await openSyncSettings(page);
  await page.getByRole('button', { name: 'Permanently delete shared game' }).click();
  await page.getByRole('button', { name: 'Delete Game permanently' }).click();
  await expectActivity(page, /Shared Game permanently deleted/);
}

export function libraryDevices(page: Page): Locator {
  return page
    .locator('section')
    .filter({ has: page.getByRole('heading', { name: 'Library devices' }) })
    .last();
}

export async function openDeviceSettings(page: Page): Promise<void> {
  await page.goto('/Settings');
  await page.getByRole('button', { name: 'Device Management' }).click();
  await expect(page.getByText('Library devices')).toBeVisible();
}

export async function removeLibraryDevice(page: Page, deviceName: string): Promise<void> {
  await openDeviceSettings(page);
  const section = libraryDevices(page);
  await expect(section.getByText(deviceName)).toBeVisible();
  // Only non-current, non-removed devices offer the action, so it is unique.
  await section.getByRole('button', { name: 'Remove device' }).click();
  await page.getByRole('dialog').getByRole('button', { name: 'Remove device' }).click();
  await expectActivity(page, /Device Profile removed/);
}

export async function rebuildCloudLibrary(page: Page): Promise<void> {
  await openSyncSettings(page);
  await page.getByRole('button', { name: 'Rebuild from this device' }).click();
  const dialog = page.getByRole('dialog');
  await dialog.getByRole('textbox').fill('yes');
  await dialog.getByRole('button', { name: 'Rebuild from this device' }).click();
  await expectActivity(page, 'Cloud Library rebuilt');
}

export async function reconnectCloudLibrary(page: Page): Promise<void> {
  await openSyncSettings(page);
  await page.getByRole('button', { name: 'Reconnect this device' }).click();
  const dialog = page.getByRole('dialog');
  await dialog.getByRole('button', { name: 'Reconnect this device' }).click();
  await expectActivity(page, 'This device reconnected');
}

export async function protectSnapshot(page: Page, snapshotId: string): Promise<void> {
  await snapshotRow(page, snapshotId).getByRole('button', { name: 'Keep' }).click();
  await page.getByRole('dialog').getByRole('button', { name: 'Keep' }).click();
  await expect(
    snapshotRow(page, snapshotId).getByRole('button', { name: /Unprotect|Remove protection/ })
  ).toBeVisible({ timeout: 15_000 });
}

export async function setSharedRetention(page: Page, limit: number): Promise<void> {
  await page.getByRole('button', { name: 'More actions' }).click();
  await page.getByRole('menuitem', { name: 'Auto-save settings' }).click();
  await expect(page.getByText('Shared automatic snapshot limit')).toBeVisible();
  const block = page
    .getByRole('heading', { name: 'Shared automatic snapshot limit' })
    .locator('xpath=ancestor::div[contains(@class,"justify-between")][1]');
  const enabled = block.getByRole('switch');
  if (!(await enabled.isChecked())) {
    await enabled.click();
  }
  await block.getByLabel('Shared automatic snapshot limit').fill(String(limit));
  await page.getByRole('button', { name: 'Save settings' }).click();
  await page.getByRole('dialog').getByRole('button', { name: 'Allow permanent cleanup' }).click();
  await expect(page.getByText('Shared automatic snapshot limit')).toBeHidden({ timeout: 15_000 });
}

export async function createPublishedSnapshot(
  page: Page,
  host: RgsmHost,
  describe: string
): Promise<string> {
  await createSnapshotViaApi(host, describe);
  const snapshotId = await latestSnapshotId(host, describe);
  await openGame(page);
  await uploadSnapshot(page, snapshotId);
  return snapshotId;
}
