import { mkdir, writeFile } from 'node:fs/promises';
import { dirname, join } from 'node:path';
import { DEVICE_A_ID, DEVICE_A_NAME, GAME_NAME } from './constants';
import { deviceLayout, type DeviceLayout } from './cloud-fixture';

export type LocalUnitSeed = {
  type: 'File' | 'Folder';
  /** Absolute path for the current device. */
  path: string;
  deleteBeforeApply?: boolean;
  enabled?: boolean;
};

export type LocalGameSeed = {
  name: string;
  units: LocalUnitSeed[];
};

export type LocalSeedOptions = {
  /** Defaults to a single Echo Keep game with one File unit at the A save path. */
  games?: LocalGameSeed[];
  /** Merged over the default settings object. */
  settings?: Record<string, unknown>;
  favorites?: unknown[];
};

function forwardSlashes(path: string): string {
  return path.replaceAll('\\', '/');
}

/**
 * Seeds a legacy-format local config with the cloud backend Disabled, so the
 * host boots into a purely local namespace. The save files themselves are not
 * created here; callers write them via `writeSaveText`.
 */
export async function seedLocalConfig(
  runRoot: string,
  options: LocalSeedOptions = {}
): Promise<DeviceLayout> {
  const device = deviceLayout(runRoot, 'a');
  const games = options.games ?? [
    { name: GAME_NAME, units: [{ type: 'File' as const, path: device.savePath }] },
  ];
  const config = {
    version: '1.9.0',
    backup_path: forwardSlashes(device.archiveRoot),
    games: games.map((game) => ({
      name: game.name,
      storage_key: game.name,
      save_paths: game.units.map((unit, index) => ({
        id: index + 1,
        unit_type: unit.type,
        paths: { [DEVICE_A_ID]: forwardSlashes(unit.path) },
        delete_before_apply: unit.deleteBeforeApply ?? false,
        enabled: unit.enabled ?? true,
      })),
      game_paths: {},
      next_save_unit_id: game.units.length + 1,
      cloud_sync_enabled: true,
    })),
    settings: {
      prompt_when_not_described: false,
      extra_backup_when_apply: true,
      confirm_before_apply_latest: false,
      confirm_before_apply_snapshot: false,
      show_edit_button: false,
      prompt_when_auto_backup: false,
      exit_to_tray: false,
      cloud_settings: {
        auto_sync_interval: 0,
        root_path: '',
        backend: { type: 'Disabled' },
        max_concurrency: 1,
      },
      locale: 'en_US',
      default_delete_before_apply: false,
      default_expend_favorites_tree: false,
      home_page: '/',
      log_to_file: true,
      add_new_to_favorites: false,
      vn_scan_dirs: [],
      save_list_expand_behavior: 'always_closed',
      save_list_last_expanded: false,
      max_auto_backup_count: 0,
      max_extra_backup_count: 5,
      appearance: { custom_font_enabled: false, ui_font_family: '' },
      compression_preset: 'Standard',
      compute_archive_hash: false,
      verify_archive_before_apply: false,
      ...(options.settings ?? {}),
    },
    favorites: options.favorites ?? [],
    quick_action: {
      quick_action_game: null,
      hotkeys: { apply: ['', '', ''], backup: ['', '', ''] },
      enable_sound: false,
      enable_notification: false,
    },
    devices: {
      [DEVICE_A_ID]: { id: DEVICE_A_ID, name: DEVICE_A_NAME, resources: [], next_resource_id: 0 },
    },
  };
  await mkdir(device.appDataDir, { recursive: true });
  await writeFile(
    join(device.appDataDir, 'GameSaveManager.config.json'),
    `${JSON.stringify(config, null, 2)}\n`,
    'utf8'
  );
  // Snapshot metadata recording expects an existing Backups.json; games added
  // through the product get one at creation time.
  for (const game of games) {
    await mkdir(join(device.archiveRoot, game.name), { recursive: true });
    await writeFile(
      join(device.archiveRoot, game.name, 'Backups.json'),
      `${JSON.stringify({ name: game.name, backups: [], device_heads: {}, sync_version: 0 }, null, 2)}\n`,
      'utf8'
    );
  }
  return device;
}

export async function writeSaveText(path: string, contents: string): Promise<void> {
  await mkdir(dirname(path), { recursive: true });
  await writeFile(path, contents, 'utf8');
}
