import { expect, type Page } from '@playwright/test';
import { DEVICE_A_ID } from './constants';
import { hostPost, type RgsmHost } from './rgsm-instance';
import { waitForFreshSnapshotSecond } from './gui';
import { waitForCommand } from './command-result';

export type LocalUnitInput = {
  type: 'File' | 'Folder';
  path: string;
  deleteBeforeApply?: boolean;
  enabled?: boolean;
};

export type LocalGame = {
  name: string;
  storage_key: string;
  save_paths: Array<{
    id: number;
    delete_before_apply: boolean;
    enabled: boolean;
    source:
      | { type: 'concrete'; unit_type: string; paths: Record<string, string> }
      | { type: 'manifestPattern'; pattern: string };
  }>;
  game_paths?: Record<string, string>;
  auto_backup?: { interval_secs: number; max_backup_count?: number | null } | null;
  cloud_sync_enabled?: boolean;
  [key: string]: unknown;
};

export async function getLocalGame(host: RgsmHost, gameName: string): Promise<LocalGame> {
  const result = await hostPost<{ games: LocalGame[] }>(host, '/api/v1/get-local-config');
  expect(result.ok, result.raw).toBe(true);
  const game = result.data.games.find((item) => item.name === gameName);
  expect(game, `game ${gameName} missing from local config`).toBeTruthy();
  return game!;
}

/** Adds a game through the same API the Add Game drawer saves to. */
export async function addGameViaApi(
  host: RgsmHost,
  gameName: string,
  units: LocalUnitInput[]
): Promise<void> {
  const result = await hostPost(host, '/api/v1/add-game', {
    game: {
      name: gameName,
      save_paths: units.map((unit, index) => ({
        id: index + 1,
        delete_before_apply: unit.deleteBeforeApply ?? false,
        enabled: unit.enabled ?? true,
        source: {
          type: 'concrete',
          unit_type: unit.type,
          paths: { [DEVICE_A_ID]: unit.path.replaceAll('\\', '/') },
        },
      })),
      game_paths: {},
    },
  });
  expect(result.ok, result.raw).toBe(true);
}

/** Replaces a game's definition (same call the save-location drawer saves with). */
export async function updateGameViaApi(
  host: RgsmHost,
  storageKey: string,
  game: LocalGame
): Promise<void> {
  const result = await hostPost(host, '/api/v1/update-game', {
    storageKey,
    game: {
      name: game.name,
      save_paths: game.save_paths,
      game_paths: game.game_paths ?? {},
    },
  });
  expect(result.ok, result.raw).toBe(true);
}

export type LocalSnapshotInfo = {
  date: string;
  describe: string;
  path: string;
  parent?: string | null;
  created_by?: string;
  archive_hash?: string | null;
  size?: number;
};

export async function listSnapshotsFor(
  host: RgsmHost,
  gameName: string
): Promise<LocalSnapshotInfo[]> {
  const game = await getLocalGame(host, gameName);
  const result = await hostPost<{ backups: LocalSnapshotInfo[] }>(
    host,
    '/api/v1/get-game-snapshots-info',
    { game }
  );
  expect(result.ok, result.raw).toBe(true);
  return result.data.backups;
}

export async function latestSnapshotIdFor(host: RgsmHost, gameName: string): Promise<string> {
  const snapshots = await listSnapshotsFor(host, gameName);
  const latest = snapshots.at(-1);
  expect(latest, `no snapshots for ${gameName}`).toBeTruthy();
  return latest!.date;
}

export async function createSnapshotForGame(
  host: RgsmHost,
  gameName: string,
  describe: string
): Promise<string> {
  await waitForFreshSnapshotSecond();
  const game = await getLocalGame(host, gameName);
  const result = await hostPost(host, '/api/v1/create-snapshot', { game, describe });
  expect(result.ok, result.raw).toBe(true);
  const snapshots = await listSnapshotsFor(host, gameName);
  const match = snapshots.find((item) => item.describe === describe);
  expect(match, `snapshot '${describe}' not created`).toBeTruthy();
  return match!.date;
}

export async function applySnapshotViaApi(
  host: RgsmHost,
  gameName: string,
  date: string
): Promise<void> {
  const game = await getLocalGame(host, gameName);
  const result = await hostPost(host, '/api/v1/restore-snapshot', { game, date });
  expect(result.ok, result.raw).toBe(true);
}

export async function getSettings(host: RgsmHost): Promise<Record<string, unknown>> {
  const result = await hostPost<{ settings: Record<string, unknown> }>(
    host,
    '/api/v1/get-local-config'
  );
  expect(result.ok, result.raw).toBe(true);
  return result.data.settings;
}

/** Writes settings back through set-config, preserving every other field. */
export async function updateSettings(
  host: RgsmHost,
  patch: Record<string, unknown>
): Promise<void> {
  const config = await hostPost<Record<string, unknown>>(host, '/api/v1/get-local-config');
  expect(config.ok, config.raw).toBe(true);
  const next = {
    ...(config.data as { settings: Record<string, unknown> }),
    settings: {
      ...(config.data as { settings: Record<string, unknown> }).settings,
      ...patch,
    },
  };
  const result = await hostPost(host, '/api/v1/set-config', { config: next });
  expect(result.ok, result.raw).toBe(true);
}

/** Clicks the in-row Delete button and confirms the local destructive dialog. */
export async function deleteSnapshotViaUi(page: Page, snapshotId: string): Promise<void> {
  const row = page.getByRole('row').filter({ hasText: snapshotId });
  await row.getByRole('button', { name: 'Delete' }).click();
  await confirmSnapshotDeletion(page, snapshotId);
}

export async function confirmSnapshotDeletion(page: Page, snapshotId: string): Promise<void> {
  const dialog = page.getByRole('dialog');
  await expect(dialog.getByText('Are you sure you want to delete?')).toBeVisible();
  await waitForCommand(page, '/api/v1/delete-snapshot', snapshotId, () =>
    dialog.getByRole('button', { name: 'Delete', exact: true }).click()
  );
  await expect(page.getByText('Successfully deleted').first()).toBeVisible({ timeout: 15_000 });
}

/** Changes a snapshot description through the row Modify button. */
export async function changeDescriptionViaUi(
  page: Page,
  snapshotId: string,
  description: string
): Promise<void> {
  const row = page.getByRole('row').filter({ hasText: snapshotId });
  await row.getByRole('button', { name: 'Modify' }).click();
  const dialog = page.getByRole('dialog', { name: 'Enter new description' });
  await expect(dialog).toBeVisible();
  await dialog.getByRole('textbox').fill(description);
  await dialog.getByRole('button', { name: 'Confirm' }).click();
  await expect(page.getByText('Successfully modified').first()).toBeVisible({ timeout: 15_000 });
}

export async function getExtraBackups(
  host: RgsmHost,
  gameName: string
): Promise<Array<{ date: string; size: number }>> {
  const game = await getLocalGame(host, gameName);
  const result = await hostPost<Array<{ date: string; size: number }>>(
    host,
    '/api/v1/get-game-extra-backups',
    { game }
  );
  expect(result.ok, result.raw).toBe(true);
  return result.data;
}
